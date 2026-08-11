use serde::{Deserialize, Serialize};

use crate::{
    FrameGate, FrameGateDecision, OcrEngine, OcrError, StabilizerUpdate, TextStabilizer,
    VisualFrame,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualPipelineStats {
    pub frames_received: u64,
    pub frames_analyzed: u64,
    pub frames_rate_limited: u64,
    pub frames_unchanged: u64,
    pub stable_regions: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualProcessOutcome {
    pub gate: FrameGateDecision,
    pub update: Option<StabilizerUpdate>,
    pub stats: VisualPipelineStats,
}

pub struct VisualPipeline<E> {
    gate: FrameGate,
    ocr: E,
    stabilizer: TextStabilizer,
    stats: VisualPipelineStats,
}

impl<E: OcrEngine> VisualPipeline<E> {
    pub fn new(gate: FrameGate, ocr: E, stabilizer: TextStabilizer) -> Self {
        Self {
            gate,
            ocr,
            stabilizer,
            stats: VisualPipelineStats::default(),
        }
    }

    pub fn process(&mut self, frame: &VisualFrame) -> Result<VisualProcessOutcome, OcrError> {
        self.stats.frames_received = self.stats.frames_received.saturating_add(1);
        let gate = self.gate.evaluate(frame);
        if !gate.should_analyze() {
            match gate {
                FrameGateDecision::RateLimited => {
                    self.stats.frames_rate_limited =
                        self.stats.frames_rate_limited.saturating_add(1);
                }
                FrameGateDecision::Unchanged { .. } => {
                    self.stats.frames_unchanged = self.stats.frames_unchanged.saturating_add(1);
                }
                FrameGateDecision::FirstFrame
                | FrameGateDecision::Changed { .. }
                | FrameGateDecision::Confirmation { .. } => {}
            }
            return Ok(VisualProcessOutcome {
                gate,
                update: None,
                stats: self.stats,
            });
        }
        self.stats.frames_analyzed = self.stats.frames_analyzed.saturating_add(1);
        let observations = self.ocr.recognize(frame)?;
        // Stabilization advances on analyzed OCR samples, not capture-frame
        // sequence numbers. Rate limiting and latest-frame replacement are
        // expected to leave gaps in the native capture sequence; treating
        // those gaps as missing OCR observations would prevent any new text
        // from ever reaching the two-sample stability threshold.
        let update = self
            .stabilizer
            .update(self.stats.frames_analyzed, observations);
        self.stats.stable_regions = update.visible.len() as u64;
        Ok(VisualProcessOutcome {
            gate,
            update: Some(update),
            stats: self.stats,
        })
    }

    pub fn source_changed_since_last_analysis(&self, frame: &VisualFrame) -> bool {
        self.gate.is_meaningfully_different(frame)
    }

    pub fn reset_text_tracks(&mut self) {
        self.stabilizer.reset();
        self.stats.stable_regions = 0;
    }
}

#[cfg(test)]
mod tests {
    use crate::{FrameGateConfig, OcrObservation, PixelFormat, TextStabilizerConfig, VisualRect};

    use super::*;

    struct FixedOcr;

    impl OcrEngine for FixedOcr {
        fn recognize(&mut self, _frame: &VisualFrame) -> Result<Vec<OcrObservation>, OcrError> {
            Ok(vec![OcrObservation {
                text: "hola".into(),
                confidence: 0.9,
                language: Some("es".into()),
                script: Some("Latn".into()),
                bounds: VisualRect {
                    x: 10.0,
                    y: 10.0,
                    width: 100.0,
                    height: 30.0,
                },
            }])
        }
    }

    fn frame(sequence: u64, timestamp: u64, shade: u8) -> VisualFrame {
        VisualFrame::new(
            sequence,
            timestamp,
            2,
            2,
            8,
            PixelFormat::Bgra8,
            [shade, shade, shade, 255].repeat(4),
        )
        .expect("frame")
    }

    #[test]
    fn static_text_is_confirmed_once_without_repeating_ocr_forever() {
        let mut pipeline = VisualPipeline::new(
            FrameGate::new(FrameGateConfig {
                minimum_interval_micros: 0,
                ..FrameGateConfig::default()
            }),
            FixedOcr,
            TextStabilizer::new(TextStabilizerConfig::default()),
        );
        let first = pipeline.process(&frame(1, 0, 40)).expect("first");
        assert!(first.update.expect("first OCR update").visible.is_empty());
        let confirmation = pipeline.process(&frame(2, 1, 40)).expect("confirmation");
        assert!(matches!(
            confirmation.gate,
            FrameGateDecision::Confirmation { .. }
        ));
        assert_eq!(
            confirmation
                .update
                .expect("confirmation OCR update")
                .visible
                .len(),
            1
        );
        let unchanged = pipeline.process(&frame(3, 2, 40)).expect("unchanged");
        assert!(matches!(
            unchanged.gate,
            FrameGateDecision::Unchanged { .. }
        ));
        assert_eq!(unchanged.stats.frames_analyzed, 2);
        assert_eq!(unchanged.stats.frames_unchanged, 1);
    }

    #[test]
    fn capture_sequence_gaps_do_not_break_text_stabilization() {
        let mut pipeline = VisualPipeline::new(
            FrameGate::new(FrameGateConfig {
                minimum_interval_micros: 0,
                change_threshold: 0.001,
                ..FrameGateConfig::default()
            }),
            FixedOcr,
            TextStabilizer::new(TextStabilizerConfig::default()),
        );
        let first = pipeline.process(&frame(1, 0, 40)).expect("first");
        assert!(first.update.expect("first OCR update").visible.is_empty());

        let second = pipeline.process(&frame(90, 1, 180)).expect("second");
        let update = second.update.expect("second OCR update");
        assert_eq!(update.visible.len(), 1);
        assert_eq!(update.translation_requests.len(), 1);
    }
}
