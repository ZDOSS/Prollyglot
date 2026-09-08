use serde::{Deserialize, Serialize};

use crate::{VisualFrame, VisualRect};

/// Compare the actual text area, rather than requiring a whole-scene change
/// before rejecting old OCR. This also validates static text after a slow pass.
pub fn text_area_changed(previous: &VisualFrame, next: &VisualFrame, bounds: VisualRect) -> bool {
    if previous.width != next.width || previous.height != next.height || !bounds.is_valid() {
        return true;
    }
    let left = (bounds.x as u32).min(previous.width);
    let top = (bounds.y as u32).min(previous.height);
    let right = ((bounds.x + bounds.width).ceil() as u32).min(previous.width);
    let bottom = ((bounds.y + bounds.height).ceil() as u32).min(previous.height);
    let (mut count, mut changed) = (0_u64, 0_u64);
    for y in top..bottom {
        for x in left..right {
            let old = y as usize * previous.stride + x as usize * 4;
            let new = y as usize * next.stride + x as usize * 4;
            let delta: u16 = (0..3)
                .map(|c| u16::from(previous.pixels()[old + c].abs_diff(next.pixels()[new + c])))
                .sum();
            changed += u64::from(delta >= 60);
            count += 1;
        }
    }
    count == 0 || changed as f32 / count as f32 >= 0.15
}

/// The inexpensive Windows capture cadence. OCR remains separately bounded.
pub const DEFAULT_LIVE_CAPTURE_FPS: u32 = 12;
pub const DEFAULT_CAPTURE_FRAME_INTERVAL_MICROS: u64 = 1_000_000 / DEFAULT_LIVE_CAPTURE_FPS as u64;
/// The maximum default cadence for expensive OCR work on changed frames.
pub const DEFAULT_OCR_INTERVAL_MICROS: u64 = 250_000;
const SUBSTANTIAL_CHANGE_THRESHOLD: f32 = 0.10;
const SUBSTANTIAL_CHANGED_SAMPLE_RATIO: f32 = 0.30;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum FrameGateDecision {
    FirstFrame,
    Changed { score: f32 },
    Confirmation { score: f32 },
    Refresh,
    Unchanged { score: f32 },
    RateLimited,
}

impl FrameGateDecision {
    pub const fn should_analyze(self) -> bool {
        matches!(
            self,
            Self::FirstFrame | Self::Changed { .. } | Self::Confirmation { .. } | Self::Refresh
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameGateConfig {
    pub minimum_interval_micros: u64,
    pub refresh_interval_micros: u64,
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
            refresh_interval_micros: 1_000_000,
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
    last_analyzed_at_micros: u64,
    accepted_fingerprint: Vec<u8>,
    awaiting_confirmation: bool,
}

impl FrameGate {
    pub fn new(config: FrameGateConfig) -> Self {
        Self {
            config,
            last_checked_at_micros: None,
            last_analyzed_at_micros: 0,
            accepted_fingerprint: Vec::new(),
            awaiting_confirmation: false,
        }
    }

    pub fn evaluate(&mut self, frame: &VisualFrame) -> FrameGateDecision {
        self.evaluate_with_pending_work(frame, false)
    }

    pub fn evaluate_with_pending_work(
        &mut self,
        frame: &VisualFrame,
        pending: bool,
    ) -> FrameGateDecision {
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
            self.last_analyzed_at_micros = frame.captured_at_micros;
            return FrameGateDecision::FirstFrame;
        }
        let (mean_difference, changed_ratio) = change_metrics(
            &self.accepted_fingerprint,
            &next,
            self.config.sample_delta_threshold,
        );
        let score = mean_difference.max(changed_ratio);
        if self.is_changed(mean_difference, changed_ratio) {
            self.last_analyzed_at_micros = frame.captured_at_micros;
            self.accepted_fingerprint = next;
            self.awaiting_confirmation = true;
            FrameGateDecision::Changed { score }
        } else if self.awaiting_confirmation {
            self.last_analyzed_at_micros = frame.captured_at_micros;
            self.awaiting_confirmation = false;
            FrameGateDecision::Confirmation { score }
        } else if pending
            || frame
                .captured_at_micros
                .saturating_sub(self.last_analyzed_at_micros)
                >= self.config.refresh_interval_micros
        {
            self.last_analyzed_at_micros = frame.captured_at_micros;
            self.accepted_fingerprint = next;
            FrameGateDecision::Refresh
        } else {
            FrameGateDecision::Unchanged { score }
        }
    }

    pub fn is_meaningfully_different(&self, frame: &VisualFrame) -> bool {
        let next = fingerprint(frame, self.config.sample_columns, self.config.sample_rows);
        if self.accepted_fingerprint.is_empty() || self.accepted_fingerprint.len() != next.len() {
            return true;
        }
        let (mean_difference, changed_ratio) = change_metrics(
            &self.accepted_fingerprint,
            &next,
            self.config.sample_delta_threshold,
        );
        self.is_changed(mean_difference, changed_ratio)
    }

    /// Returns true only when a newer frame represents a broad scene change.
    ///
    /// The ordinary change gate is intentionally sensitive enough to notice a
    /// small subtitle. Reusing that threshold to reject a slow OCR result would
    /// classify cursor movement, counters, and video controls as a new scene
    /// and repeatedly clear otherwise useful translations.
    pub fn is_substantially_different(&self, frame: &VisualFrame) -> bool {
        let next = fingerprint(frame, self.config.sample_columns, self.config.sample_rows);
        if self.accepted_fingerprint.is_empty() || self.accepted_fingerprint.len() != next.len() {
            return true;
        }
        let (mean_difference, changed_ratio) = change_metrics(
            &self.accepted_fingerprint,
            &next,
            self.config.sample_delta_threshold,
        );
        mean_difference >= SUBSTANTIAL_CHANGE_THRESHOLD
            || changed_ratio >= SUBSTANTIAL_CHANGED_SAMPLE_RATIO
    }

    fn is_changed(&self, mean_difference: f32, changed_ratio: f32) -> bool {
        mean_difference >= self.config.change_threshold
            || changed_ratio >= self.config.changed_sample_ratio
    }
}

fn change_metrics(previous: &[u8], next: &[u8], sample_delta_threshold: u8) -> (f32, f32) {
    let total_difference = previous
        .iter()
        .zip(next)
        .map(|(previous, current)| previous.abs_diff(*current) as f32)
        .sum::<f32>();
    let changed_samples = previous
        .iter()
        .zip(next)
        .filter(|(previous, current)| previous.abs_diff(**current) >= sample_delta_threshold)
        .count();
    (
        total_difference / (next.len().max(1) as f32 * 255.0),
        changed_samples as f32 / next.len().max(1) as f32,
    )
}

fn fingerprint(frame: &VisualFrame, requested_columns: u16, requested_rows: u16) -> Vec<u8> {
    let columns = u32::from(requested_columns).max(1).min(frame.width);
    let rows = u32::from(requested_rows).max(1).min(frame.height);
    let mut result = Vec::with_capacity((columns * rows * 3) as usize);
    for sample_y in 0..rows {
        let top = (u64::from(sample_y) * u64::from(frame.height) / u64::from(rows)) as usize;
        let bottom = (u64::from(sample_y + 1) * u64::from(frame.height) / u64::from(rows)) as usize;
        for sample_x in 0..columns {
            let left = (u64::from(sample_x) * u64::from(frame.width) / u64::from(columns)) as usize;
            let right =
                (u64::from(sample_x + 1) * u64::from(frame.width) / u64::from(columns)) as usize;
            let (mut sum, mut minimum, mut maximum) = (0_u64, 255_u8, 0_u8);
            // Aggregate the whole tile: thin subtitle strokes can fall entirely
            // between isolated sample points, particularly on a 4K display.
            for y in top..bottom {
                for x in left..right {
                    let offset = y * frame.stride + x * frame.pixel_format.bytes_per_pixel();
                    let pixel = &frame.pixels()[offset..offset + 3];
                    let value = ((29 * u16::from(pixel[0])
                        + 150 * u16::from(pixel[1])
                        + 77 * u16::from(pixel[2]))
                        >> 8) as u8;
                    sum += u64::from(value);
                    minimum = minimum.min(value);
                    maximum = maximum.max(value);
                }
            }
            result.extend([
                (sum / ((bottom - top) * (right - left)) as u64) as u8,
                minimum,
                maximum,
            ]);
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
    fn thin_subtitles_between_old_sample_rows_trigger_ocr_and_local_staleness() {
        let (width, height) = (1920, 1080);
        let blank = vec![0; width * height * 4];
        let mut pixels = blank.clone();
        for y in 994..1018 {
            for x in 510..1410 {
                pixels[(y * width + x) * 4..(y * width + x) * 4 + 3].fill(255);
            }
        }
        let frame = |time, pixels| {
            VisualFrame::new(
                time,
                time,
                width as u32,
                height as u32,
                width * 4,
                PixelFormat::Bgra8,
                pixels,
            )
            .unwrap()
        };
        let first = frame(0, blank.clone());
        let mut gate = FrameGate::new(FrameGateConfig::default());
        gate.evaluate(&first);
        gate.evaluate(&frame(300_000, blank));
        let changed = frame(600_000, pixels);
        assert!(matches!(
            gate.evaluate(&changed),
            FrameGateDecision::Changed { .. }
        ));
        assert!(text_area_changed(
            &changed,
            &first,
            VisualRect {
                x: 510.0,
                y: 994.0,
                width: 900.0,
                height: 24.0
            }
        ));
    }

    #[test]
    fn static_frames_receive_a_bounded_refresh_even_without_a_change_signal() {
        let mut gate = FrameGate::new(FrameGateConfig::default());
        gate.evaluate(&solid(1, 0, 20));
        gate.evaluate(&solid(2, 300_000, 20));
        assert!(matches!(
            gate.evaluate(&solid(3, 600_000, 20)),
            FrameGateDecision::Unchanged { .. }
        ));
        assert_eq!(
            gate.evaluate(&solid(4, 1_300_000, 20)),
            FrameGateDecision::Refresh
        );
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

    #[test]
    fn compares_a_newer_frame_without_advancing_the_gate() {
        let mut gate = FrameGate::new(FrameGateConfig::default());
        gate.evaluate(&solid(1, 0, 20));
        assert!(!gate.is_meaningfully_different(&solid(2, 300_000, 22)));
        assert!(gate.is_meaningfully_different(&solid(3, 600_000, 180)));
        assert!(matches!(
            gate.evaluate(&solid(4, 900_000, 22)),
            FrameGateDecision::Confirmation { .. }
        ));
    }

    #[test]
    fn substantial_change_ignores_small_text_like_updates_but_detects_a_new_scene() {
        let width = 64_u32;
        let height = 36_u32;
        let base = [16, 16, 16, 255].repeat((width * height) as usize);
        let mut localized = base.clone();
        for y in 16_usize..20 {
            for x in 10_usize..30 {
                let offset = (y * width as usize + x) * 4;
                localized[offset..offset + 3].fill(240);
            }
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

        let localized = frame(2, 300_000, localized);
        assert!(gate.is_meaningfully_different(&localized));
        assert!(!gate.is_substantially_different(&localized));
        assert!(gate.is_substantially_different(&frame(
            3,
            600_000,
            [240, 240, 240, 255].repeat((width * height) as usize)
        )));
    }
}
