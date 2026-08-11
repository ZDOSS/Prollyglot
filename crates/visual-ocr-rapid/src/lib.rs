//! RapidOCR/PP-OCRv6 adapter for transient Prollyglot visual frames.
//!
//! The adapter accepts in-memory BGRA frames and returns only recognized text,
//! confidence, and capture-space geometry. It never writes source frames to
//! disk and never includes pixels in an error or diagnostic value.

use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use image::RgbImage;
use prollyglot_visual_pipeline::{
    OcrEngine, OcrError, OcrObservation, PixelFormat, VisualFrame, VisualRect,
};
use rapidocr_core::{
    OcrCancellationToken, RapidOcr,
    config::{LimitType, PipelineConfig, RapidOcrConfig},
    is_cancelled_error,
    types::Quad,
};

const LIVE_OCR_MAX_SIDE: u32 = 1_280;
const FOCUSED_RESULT_LIMIT: usize = 6;

pub struct RapidOcrEngine {
    runner: RapidOcr,
    language_hint: String,
    profile: RecognitionProfile,
    cancellation: RapidOcrCancellation,
}

#[derive(Clone, Default)]
pub struct RapidOcrCancellation {
    state: Arc<RapidOcrCancellationState>,
}

#[derive(Default)]
struct RapidOcrCancellationState {
    shutdown_requested: AtomicBool,
    active: Mutex<Option<OcrCancellationToken>>,
}

impl RapidOcrCancellation {
    pub fn cancel(&self) {
        self.state.shutdown_requested.store(true, Ordering::Release);
        let active = self
            .state
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if let Some(active) = active {
            active.cancel();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.shutdown_requested.load(Ordering::Acquire)
    }

    fn begin(&self, token: OcrCancellationToken) {
        let mut active = self
            .state
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.is_cancelled() {
            token.cancel();
        }
        *active = Some(token);
    }

    fn finish(&self) {
        self.state
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RecognitionProfile {
    #[default]
    Focused,
    AllText,
}

impl RapidOcrEngine {
    pub fn load(
        model_directory: impl AsRef<Path>,
        language_hint: impl Into<String>,
    ) -> Result<Self, OcrError> {
        Self::load_with_profile(model_directory, language_hint, RecognitionProfile::Focused)
    }

    pub fn load_with_profile(
        model_directory: impl AsRef<Path>,
        language_hint: impl Into<String>,
        profile: RecognitionProfile,
    ) -> Result<Self, OcrError> {
        let mut config = RapidOcrConfig::ppocr_v6_small(model_directory.as_ref());
        // Desktop text is expected to be upright. Avoiding the per-crop direction
        // classifier and bounding detector input keeps live video responsive.
        config.pipeline = PipelineConfig::without_cls();
        config.max_side_len = LIVE_OCR_MAX_SIDE;
        config.inference.intra_threads = std::thread::available_parallelism()
            .map(|threads| threads.get())
            .unwrap_or(2)
            .clamp(1, 4);
        config.inference.enable_cpu_mem_arena = true;
        if let Some(detector) = &mut config.det {
            detector.limit_type = LimitType::Max;
            detector.limit_side_len = LIVE_OCR_MAX_SIDE;
            detector.max_candidates = 128;
        }
        let runner = RapidOcr::new(config).map_err(|error| {
            OcrError::Unavailable(format!("PP-OCRv6 Small could not load: {error:#}"))
        })?;
        Ok(Self {
            runner,
            language_hint: language_hint.into(),
            profile,
            cancellation: RapidOcrCancellation::default(),
        })
    }

    pub fn cancellation(&self) -> RapidOcrCancellation {
        self.cancellation.clone()
    }
}

impl OcrEngine for RapidOcrEngine {
    fn recognize(&mut self, frame: &VisualFrame) -> Result<Vec<OcrObservation>, OcrError> {
        let cancellation = OcrCancellationToken::new();
        self.cancellation.begin(cancellation.clone());
        let result = self.recognize_cancellable(frame, &cancellation);
        self.cancellation.finish();
        result
    }
}

impl RapidOcrEngine {
    fn recognize_cancellable(
        &mut self,
        frame: &VisualFrame,
        cancellation: &OcrCancellationToken,
    ) -> Result<Vec<OcrObservation>, OcrError> {
        cancellation.checkpoint().map_err(|_| OcrError::Cancelled)?;
        let image = frame_to_rgb_cancellable(frame, cancellation)?;
        let output = self
            .runner
            .run_image_cancellable(&image, cancellation)
            .map_err(|error| {
                if is_cancelled_error(&error) || cancellation.is_cancelled() {
                    OcrError::Cancelled
                } else {
                    OcrError::Inference(format!("PP-OCRv6 inference failed: {error:#}"))
                }
            })?;
        let observations = output
            .lines
            .into_iter()
            .filter_map(|line| {
                let bounds = quad_bounds(&line.bbox)?;
                Some(OcrObservation {
                    text: line.text,
                    confidence: line.score,
                    language: Some(self.language_hint.clone()),
                    script: None,
                    bounds,
                })
            })
            .collect();
        Ok(prepare_observations(
            observations,
            frame.width,
            frame.height,
            &self.language_hint,
            self.profile,
        ))
    }
}

fn prepare_observations(
    observations: Vec<OcrObservation>,
    frame_width: u32,
    frame_height: u32,
    language: &str,
    profile: RecognitionProfile,
) -> Vec<OcrObservation> {
    let mut useful: Vec<_> = merge_nearby_lines(observations)
        .into_iter()
        .filter_map(|mut observation| {
            if !observation_is_useful(
                &observation.text,
                observation.confidence,
                observation.bounds,
                frame_width,
                frame_height,
                language,
                profile,
            ) {
                return None;
            }
            observation.script = dominant_script(&observation.text).map(str::to_owned);
            Some(observation)
        })
        .collect();
    if profile == RecognitionProfile::Focused && useful.len() > FOCUSED_RESULT_LIMIT {
        useful.sort_by(|left, right| {
            observation_priority(right, frame_width, frame_height).total_cmp(&observation_priority(
                left,
                frame_width,
                frame_height,
            ))
        });
        useful.truncate(FOCUSED_RESULT_LIMIT);
    }
    useful.sort_by(|left, right| {
        left.bounds
            .y
            .total_cmp(&right.bounds.y)
            .then_with(|| left.bounds.x.total_cmp(&right.bounds.x))
    });
    useful
}

fn merge_nearby_lines(observations: Vec<OcrObservation>) -> Vec<OcrObservation> {
    if observations.len() < 2 {
        return observations;
    }
    let mut parents: Vec<usize> = (0..observations.len()).collect();
    for left in 0..observations.len() {
        for right in (left + 1)..observations.len() {
            if lines_belong_together(observations[left].bounds, observations[right].bounds) {
                union(&mut parents, left, right);
            }
        }
    }

    let mut groups = std::collections::BTreeMap::<usize, Vec<OcrObservation>>::new();
    for (index, observation) in observations.into_iter().enumerate() {
        let root = find(&mut parents, index);
        groups.entry(root).or_default().push(observation);
    }
    groups
        .into_values()
        .map(|mut lines| {
            lines.sort_by(|left, right| {
                left.bounds
                    .y
                    .total_cmp(&right.bounds.y)
                    .then_with(|| left.bounds.x.total_cmp(&right.bounds.x))
            });
            let bounds = lines
                .iter()
                .map(|line| line.bounds)
                .reduce(union_bounds)
                .expect("a grouped OCR observation has at least one line");
            let total_weight: usize = lines
                .iter()
                .map(|line| {
                    line.text
                        .chars()
                        .filter(|character| !character.is_whitespace())
                        .count()
                        .max(1)
                })
                .sum();
            let confidence = lines
                .iter()
                .map(|line| {
                    let weight = line
                        .text
                        .chars()
                        .filter(|character| !character.is_whitespace())
                        .count()
                        .max(1);
                    line.confidence * weight as f32
                })
                .sum::<f32>()
                / total_weight as f32;
            OcrObservation {
                text: lines
                    .iter()
                    .map(|line| line.text.trim())
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "),
                confidence,
                language: lines.first().and_then(|line| line.language.clone()),
                script: None,
                bounds,
            }
        })
        .collect()
}

fn lines_belong_together(left: VisualRect, right: VisualRect) -> bool {
    let left_right = left.x + left.width;
    let right_right = right.x + right.width;
    let left_bottom = left.y + left.height;
    let right_bottom = right.y + right.height;
    let horizontal_gap = (left.x - right_right).max(right.x - left_right).max(0.0);
    let vertical_gap = (left.y - right_bottom).max(right.y - left_bottom).max(0.0);
    let horizontal_overlap = (left_right.min(right_right) - left.x.max(right.x)).max(0.0);
    let vertical_overlap = (left_bottom.min(right_bottom) - left.y.max(right.y)).max(0.0);
    let minimum_width = left.width.min(right.width).max(1.0);
    let minimum_height = left.height.min(right.height).max(1.0);
    let maximum_height = left.height.max(right.height);
    let center_distance = ((left.x + left.width / 2.0) - (right.x + right.width / 2.0)).abs();

    let same_line =
        vertical_overlap / minimum_height >= 0.58 && horizontal_gap <= maximum_height * 1.15;
    let stacked_lines = vertical_gap <= maximum_height * 0.62
        && (horizontal_overlap / minimum_width >= 0.45
            || center_distance <= left.width.max(right.width) * 0.18);
    same_line || stacked_lines
}

fn union_bounds(left: VisualRect, right: VisualRect) -> VisualRect {
    let x = left.x.min(right.x);
    let y = left.y.min(right.y);
    let right_edge = (left.x + left.width).max(right.x + right.width);
    let bottom = (left.y + left.height).max(right.y + right.height);
    VisualRect {
        x,
        y,
        width: right_edge - x,
        height: bottom - y,
    }
}

fn find(parents: &mut [usize], index: usize) -> usize {
    let parent = parents[index];
    if parent != index {
        parents[index] = find(parents, parent);
    }
    parents[index]
}

fn union(parents: &mut [usize], left: usize, right: usize) {
    let left_root = find(parents, left);
    let right_root = find(parents, right);
    if left_root != right_root {
        parents[right_root] = left_root;
    }
}

fn observation_priority(observation: &OcrObservation, frame_width: u32, frame_height: u32) -> f32 {
    let frame_width = frame_width.max(1) as f32;
    let frame_height = frame_height.max(1) as f32;
    let relative_height = observation.bounds.height / frame_height;
    let relative_area = observation.bounds.area() / (frame_width * frame_height);
    let center_x = (observation.bounds.x + observation.bounds.width / 2.0) / frame_width;
    let center_y = (observation.bounds.y + observation.bounds.height / 2.0) / frame_height;
    let center_distance = ((center_x - 0.5).powi(2) + (center_y - 0.5).powi(2)).sqrt();
    observation.confidence * 2.0
        + relative_height * 30.0
        + relative_area * 8.0
        + (1.0 - center_distance.min(1.0)) * 0.2
}

fn observation_is_useful(
    text: &str,
    confidence: f32,
    bounds: VisualRect,
    frame_width: u32,
    frame_height: u32,
    language: &str,
    profile: RecognitionProfile,
) -> bool {
    let trimmed = text.trim();
    let threshold = match profile {
        RecognitionProfile::Focused => 0.62,
        RecognitionProfile::AllText => 0.46,
    };
    if !confidence.is_finite() || confidence < threshold {
        return false;
    }
    let non_whitespace = trimmed
        .chars()
        .filter(|character| !character.is_whitespace())
        .count();
    let signal = trimmed
        .chars()
        .filter(|character| character.is_alphabetic() || character.is_numeric())
        .count();
    let minimum_characters = if matches!(expected_script(language), ScriptFamily::Latin) {
        3
    } else {
        2
    };
    if non_whitespace < minimum_characters
        || signal * 100 < non_whitespace.saturating_mul(55)
        || !matches_expected_script(trimmed, language)
    {
        return false;
    }
    let mut unique = std::collections::HashSet::new();
    for character in trimmed
        .chars()
        .filter(|character| character.is_alphabetic() || character.is_numeric())
    {
        unique.insert(character.to_lowercase().next().unwrap_or(character));
    }
    if signal >= 4 && unique.len() <= 1 {
        return false;
    }
    if profile == RecognitionProfile::Focused {
        let lowercase = trimmed.to_lowercase();
        if lowercase.contains("http://")
            || lowercase.contains("https://")
            || lowercase.starts_with("www.")
        {
            return false;
        }
        let minimum_height = (frame_height as f32 * 0.02).clamp(16.0, 32.0);
        let wide_prominent =
            bounds.width >= frame_width as f32 * 0.2 && bounds.height >= minimum_height * 0.72;
        if bounds.height < minimum_height && !wide_prominent {
            return false;
        }
    } else if bounds.height < 8.0 {
        return false;
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScriptFamily {
    Latin,
    Cyrillic,
    Arabic,
    Bengali,
    Devanagari,
    Han,
    Japanese,
    Korean,
}

fn expected_script(language: &str) -> ScriptFamily {
    match language {
        "bg" | "ru" | "uk" => ScriptFamily::Cyrillic,
        "ar" => ScriptFamily::Arabic,
        "bn" => ScriptFamily::Bengali,
        "hi" => ScriptFamily::Devanagari,
        "zh" => ScriptFamily::Han,
        "ja" => ScriptFamily::Japanese,
        "ko" => ScriptFamily::Korean,
        _ => ScriptFamily::Latin,
    }
}

fn matches_expected_script(text: &str, language: &str) -> bool {
    let expected = expected_script(language);
    let letters: Vec<char> = text
        .chars()
        .filter(|character| character.is_alphabetic())
        .collect();
    if letters.is_empty() {
        return false;
    }
    let matches = letters
        .iter()
        .filter(|character| character_matches_script(**character, expected))
        .count();
    matches * 100 >= letters.len().saturating_mul(35)
}

fn character_matches_script(character: char, script: ScriptFamily) -> bool {
    let code = character as u32;
    match script {
        ScriptFamily::Latin => {
            character.is_ascii_alphabetic()
                || (0x00c0..=0x024f).contains(&code)
                || (0x1e00..=0x1eff).contains(&code)
        }
        ScriptFamily::Cyrillic => (0x0400..=0x052f).contains(&code),
        ScriptFamily::Arabic => {
            (0x0600..=0x06ff).contains(&code) || (0x0750..=0x077f).contains(&code)
        }
        ScriptFamily::Bengali => (0x0980..=0x09ff).contains(&code),
        ScriptFamily::Devanagari => (0x0900..=0x097f).contains(&code),
        ScriptFamily::Han => (0x3400..=0x4dbf).contains(&code) || (0x4e00..=0x9fff).contains(&code),
        ScriptFamily::Japanese => {
            (0x3040..=0x30ff).contains(&code)
                || (0x3400..=0x4dbf).contains(&code)
                || (0x4e00..=0x9fff).contains(&code)
        }
        ScriptFamily::Korean => {
            (0x1100..=0x11ff).contains(&code)
                || (0x3130..=0x318f).contains(&code)
                || (0xac00..=0xd7af).contains(&code)
                || (0x4e00..=0x9fff).contains(&code)
        }
    }
}

fn dominant_script(text: &str) -> Option<&'static str> {
    let families = [
        (ScriptFamily::Japanese, "Jpan"),
        (ScriptFamily::Korean, "Kore"),
        (ScriptFamily::Han, "Hani"),
        (ScriptFamily::Cyrillic, "Cyrl"),
        (ScriptFamily::Arabic, "Arab"),
        (ScriptFamily::Bengali, "Beng"),
        (ScriptFamily::Devanagari, "Deva"),
        (ScriptFamily::Latin, "Latn"),
    ];
    families
        .into_iter()
        .map(|(family, label)| {
            (
                text.chars()
                    .filter(|character| character_matches_script(*character, family))
                    .count(),
                label,
            )
        })
        .filter(|(count, _)| *count > 0)
        .max_by_key(|(count, _)| *count)
        .map(|(_, label)| label)
}

#[cfg(test)]
fn frame_to_rgb(frame: &VisualFrame) -> Result<RgbImage, OcrError> {
    frame_to_rgb_cancellable(frame, &OcrCancellationToken::new())
}

fn frame_to_rgb_cancellable(
    frame: &VisualFrame,
    cancellation: &OcrCancellationToken,
) -> Result<RgbImage, OcrError> {
    if frame.pixel_format != PixelFormat::Bgra8 {
        return Err(OcrError::Inference(
            "unsupported visual pixel format".into(),
        ));
    }
    let width = usize::try_from(frame.width)
        .map_err(|_| OcrError::Inference("visual frame width is too large".into()))?;
    let height = usize::try_from(frame.height)
        .map_err(|_| OcrError::Inference("visual frame height is too large".into()))?;
    let pixel_count = width
        .checked_mul(height)
        .ok_or_else(|| OcrError::Inference("visual frame dimensions overflowed".into()))?;
    let rgb_length = pixel_count
        .checked_mul(3)
        .ok_or_else(|| OcrError::Inference("visual image size overflowed".into()))?;
    let mut rgb = vec![0_u8; rgb_length];
    for row in 0..height {
        if row.is_multiple_of(32) {
            cancellation.checkpoint().map_err(|_| OcrError::Cancelled)?;
        }
        let source_row = row * frame.stride;
        let target_row = row * width * 3;
        for column in 0..width {
            let source = source_row + column * 4;
            let target = target_row + column * 3;
            rgb[target] = frame.pixels()[source + 2];
            rgb[target + 1] = frame.pixels()[source + 1];
            rgb[target + 2] = frame.pixels()[source];
        }
    }
    RgbImage::from_raw(frame.width, frame.height, rgb)
        .ok_or_else(|| OcrError::Inference("visual frame could not be converted for OCR".into()))
}

fn quad_bounds(quad: &Quad) -> Option<VisualRect> {
    let min_x = quad.points.iter().map(|point| point[0]).reduce(f32::min)?;
    let min_y = quad.points.iter().map(|point| point[1]).reduce(f32::min)?;
    let max_x = quad.points.iter().map(|point| point[0]).reduce(f32::max)?;
    let max_y = quad.points.iter().map(|point| point[1]).reduce(f32::max)?;
    let bounds = VisualRect {
        x: min_x.max(0.0),
        y: min_y.max(0.0),
        width: max_x - min_x,
        height: max_y - min_y,
    };
    bounds.is_valid().then_some(bounds)
}

#[cfg(test)]
mod tests {
    use prollyglot_visual_pipeline::VisualFrame;

    use super::*;

    #[test]
    fn converts_strided_bgra_to_packed_rgb() {
        let frame = VisualFrame::new(
            1,
            0,
            2,
            1,
            12,
            PixelFormat::Bgra8,
            vec![3, 2, 1, 255, 30, 20, 10, 255, 0, 0, 0, 0],
        )
        .expect("frame");
        let image = frame_to_rgb(&frame).expect("RGB image");
        assert_eq!(image.as_raw(), &[1, 2, 3, 10, 20, 30]);
    }

    #[test]
    fn cancellation_is_terminal_for_the_live_ocr_session() {
        let cancellation = RapidOcrCancellation::default();
        cancellation.cancel();
        let request = OcrCancellationToken::new();
        cancellation.begin(request.clone());
        assert!(cancellation.is_cancelled());
        assert!(request.is_cancelled());
        cancellation.finish();
    }

    #[test]
    fn converts_quad_to_axis_aligned_capture_bounds() {
        let bounds = quad_bounds(&Quad {
            points: [[10.0, 14.0], [80.0, 10.0], [82.0, 34.0], [12.0, 38.0]],
        })
        .expect("bounds");
        assert_eq!(bounds.x, 10.0);
        assert_eq!(bounds.y, 10.0);
        assert_eq!(bounds.width, 72.0);
        assert_eq!(bounds.height, 28.0);
    }

    #[test]
    fn focused_profile_rejects_low_confidence_and_small_interface_copy() {
        let ordinary_bounds = VisualRect {
            x: 100.0,
            y: 100.0,
            width: 420.0,
            height: 42.0,
        };
        assert!(!observation_is_useful(
            "Breaking news",
            0.54,
            ordinary_bounds,
            1_920,
            1_080,
            "en",
            RecognitionProfile::Focused,
        ));
        assert!(!observation_is_useful(
            "Settings",
            0.94,
            VisualRect {
                width: 90.0,
                height: 10.0,
                ..ordinary_bounds
            },
            1_920,
            1_080,
            "en",
            RecognitionProfile::Focused,
        ));
    }

    #[test]
    fn focused_profile_keeps_prominent_source_language_text() {
        assert!(observation_is_useful(
            "新しい計画が発表されました",
            0.91,
            VisualRect {
                x: 320.0,
                y: 820.0,
                width: 720.0,
                height: 48.0,
            },
            1_920,
            1_080,
            "ja",
            RecognitionProfile::Focused,
        ));
    }

    #[test]
    fn merges_a_stacked_sign_into_one_translation_phrase() {
        let line = |text: &str, x: f32, y: f32, width: f32| OcrObservation {
            text: text.into(),
            confidence: 0.92,
            language: Some("es".into()),
            script: None,
            bounds: VisualRect {
                x,
                y,
                width,
                height: 20.0,
            },
        };
        let merged = merge_nearby_lines(vec![
            line("TIERRA", 110.0, 100.0, 80.0),
            line("DE AMOR", 102.0, 122.0, 96.0),
            line("Y CORAJE", 98.0, 144.0, 104.0),
            line("Share", 600.0, 400.0, 70.0),
        ]);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|region| {
            region.text == "TIERRA DE AMOR Y CORAJE" && region.bounds.height == 64.0
        }));
    }

    #[test]
    fn focused_results_are_bounded_to_prominent_regions() {
        let observations = (0..12)
            .map(|index| OcrObservation {
                text: format!("Spanish caption number {index}"),
                confidence: 0.9,
                language: Some("es".into()),
                script: None,
                bounds: VisualRect {
                    x: 120.0,
                    y: 50.0 + index as f32 * 100.0,
                    width: 240.0,
                    height: 50.0 + index as f32,
                },
            })
            .collect();
        let prepared = prepare_observations(
            observations,
            1_920,
            2_000,
            "es",
            RecognitionProfile::Focused,
        );
        assert_eq!(prepared.len(), FOCUSED_RESULT_LIMIT);
    }

    #[test]
    fn source_language_filter_rejects_unrelated_interface_script() {
        assert!(!observation_is_useful(
            "Share this video",
            0.98,
            VisualRect {
                x: 1_500.0,
                y: 80.0,
                width: 260.0,
                height: 36.0,
            },
            1_920,
            1_080,
            "ja",
            RecognitionProfile::AllText,
        ));
    }

    #[test]
    fn all_text_profile_keeps_small_requested_copy() {
        assert!(observation_is_useful(
            "Menu item",
            0.72,
            VisualRect {
                x: 50.0,
                y: 50.0,
                width: 100.0,
                height: 10.0,
            },
            1_920,
            1_080,
            "en",
            RecognitionProfile::AllText,
        ));
    }

    #[test]
    #[ignore = "requires a downloaded PP-OCRv6 model directory"]
    fn loads_verified_ppocrv6_artifacts() {
        let directory = std::env::var_os("PROLLYGLOT_VISUAL_OCR_MODEL_DIR")
            .expect("set PROLLYGLOT_VISUAL_OCR_MODEL_DIR");
        RapidOcrEngine::load(directory, "ja").expect("PP-OCRv6 model should initialize");
    }
}
