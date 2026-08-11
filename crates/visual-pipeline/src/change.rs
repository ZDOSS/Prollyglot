use serde::{Deserialize, Serialize};

use crate::VisualFrame;

/// The inexpensive Windows capture cadence. OCR remains separately bounded.
pub const DEFAULT_LIVE_CAPTURE_FPS: u32 = 12;
pub const DEFAULT_CAPTURE_FRAME_INTERVAL_MICROS: u64 = 1_000_000 / DEFAULT_LIVE_CAPTURE_FPS as u64;
/// The maximum default cadence for expensive OCR work on changed frames.
pub const DEFAULT_OCR_INTERVAL_MICROS: u64 = 250_000;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum FrameGateDecision {
    FirstFrame,
    Changed { score: f32 },
    Confirmation { score: f32 },
    Unchanged { score: f32 },
    RateLimited,
}

impl FrameGateDecision {
    pub const fn should_analyze(self) -> bool {
        matches!(
            self,
            Self::FirstFrame | Self::Changed { .. } | Self::Confirmation { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameGateConfig {
    pub minimum_interval_micros: u64,
    pub change_threshold: f32,
    pub changed_sample_ratio: f32,
    pub sample_delta_threshold: u8,
    pub sample_columns: u16,
    pub sample_rows: u16,
}

impl Default for FrameGateConfig {
    fn default() -> Self {
        Self {
            minimum_interval_micros: DEFAULT_OCR_INTERVAL_MICROS,
            change_threshold: 0.012,
            changed_sample_ratio: 0.006,
            sample_delta_threshold: 20,
            sample_columns: 64,
            sample_rows: 36,
        }
    }
}

pub struct FrameGate {
    config: FrameGateConfig,
    last_checked_at_micros: Option<u64>,
    accepted_fingerprint: Vec<u8>,
    awaiting_confirmation: bool,
}

impl FrameGate {
    pub fn new(config: FrameGateConfig) -> Self {
        Self {
            config,
            last_checked_at_micros: None,
            accepted_fingerprint: Vec::new(),
            awaiting_confirmation: false,
        }
    }

    pub fn evaluate(&mut self, frame: &VisualFrame) -> FrameGateDecision {
        if self.last_checked_at_micros.is_some_and(|last| {
            frame.captured_at_micros.saturating_sub(last) < self.config.minimum_interval_micros
        }) {
            return FrameGateDecision::RateLimited;
        }
        self.last_checked_at_micros = Some(frame.captured_at_micros);
        let next = fingerprint(frame, self.config.sample_columns, self.config.sample_rows);
        if self.accepted_fingerprint.len() != next.len() || self.accepted_fingerprint.is_empty() {
            self.accepted_fingerprint = next;
            self.awaiting_confirmation = true;
            return FrameGateDecision::FirstFrame;
        }
        let total_difference = self
            .accepted_fingerprint
            .iter()
            .zip(&next)
            .map(|(previous, current)| previous.abs_diff(*current) as f32)
            .sum::<f32>();
        let changed_samples = self
            .accepted_fingerprint
            .iter()
            .zip(&next)
            .filter(|(previous, current)| {
                previous.abs_diff(**current) >= self.config.sample_delta_threshold
            })
            .count();
        let mean_difference = total_difference / (next.len() as f32 * 255.0);
        let changed_ratio = changed_samples as f32 / next.len() as f32;
        let score = mean_difference.max(changed_ratio);
        if mean_difference >= self.config.change_threshold
            || changed_ratio >= self.config.changed_sample_ratio
        {
            self.accepted_fingerprint = next;
            self.awaiting_confirmation = true;
            FrameGateDecision::Changed { score }
        } else if self.awaiting_confirmation {
            self.awaiting_confirmation = false;
            FrameGateDecision::Confirmation { score }
        } else {
            FrameGateDecision::Unchanged { score }
        }
    }
}

fn fingerprint(frame: &VisualFrame, requested_columns: u16, requested_rows: u16) -> Vec<u8> {
    let columns = u32::from(requested_columns).max(1).min(frame.width);
    let rows = u32::from(requested_rows).max(1).min(frame.height);
    let mut result = Vec::with_capacity((columns * rows) as usize);
    for sample_y in 0..rows {
        let y = ((u64::from(sample_y) * u64::from(frame.height)) / u64::from(rows)) as usize;
        for sample_x in 0..columns {
            let x = ((u64::from(sample_x) * u64::from(frame.width)) / u64::from(columns)) as usize;
            let offset = y * frame.stride + x * frame.pixel_format.bytes_per_pixel();
            let blue = u16::from(frame.pixels()[offset]);
            let green = u16::from(frame.pixels()[offset + 1]);
            let red = u16::from(frame.pixels()[offset + 2]);
            result.push(((29 * blue + 150 * green + 77 * red) >> 8) as u8);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use crate::{PixelFormat, VisualFrame};

    use super::*;

    fn solid(sequence: u64, captured_at_micros: u64, value: u8) -> VisualFrame {
        let pixel = [value, value, value, 255];
        VisualFrame::new(
            sequence,
            captured_at_micros,
            4,
            4,
            16,
            PixelFormat::Bgra8,
            pixel.repeat(16),
        )
        .expect("frame")
    }

    #[test]
    fn rate_limits_before_comparing_pixels() {
        let mut gate = FrameGate::new(FrameGateConfig::default());
        assert_eq!(
            gate.evaluate(&solid(1, 0, 0)),
            FrameGateDecision::FirstFrame
        );
        assert_eq!(
            gate.evaluate(&solid(2, 100_000, 255)),
            FrameGateDecision::RateLimited
        );
    }

    #[test]
    fn confirms_once_then_skips_static_frames_and_admits_meaningful_change() {
        let mut gate = FrameGate::new(FrameGateConfig::default());
        gate.evaluate(&solid(1, 0, 20));
        assert!(matches!(
            gate.evaluate(&solid(2, 300_000, 22)),
            FrameGateDecision::Confirmation { .. }
        ));
        assert!(matches!(
            gate.evaluate(&solid(3, 600_000, 22)),
            FrameGateDecision::Unchanged { .. }
        ));
        assert!(matches!(
            gate.evaluate(&solid(4, 900_000, 180)),
            FrameGateDecision::Changed { .. }
        ));
    }

    #[test]
    fn detects_a_small_localized_text_like_change() {
        let width = 64_u32;
        let height = 36_u32;
        let base = [0, 0, 0, 255].repeat((width * height) as usize);
        let mut changed = base.clone();
        for x in 12_usize..28 {
            let offset = (18 * width as usize + x) * 4;
            changed[offset..offset + 3].fill(255);
        }
        let frame = |sequence, captured_at_micros, pixels| {
            VisualFrame::new(
                sequence,
                captured_at_micros,
                width,
                height,
                width as usize * 4,
                PixelFormat::Bgra8,
                pixels,
            )
            .expect("frame")
        };
        let mut gate = FrameGate::new(FrameGateConfig::default());
        gate.evaluate(&frame(1, 0, base));
        assert!(matches!(
            gate.evaluate(&frame(2, 300_000, changed)),
            FrameGateDecision::Changed { .. }
        ));
    }
}
