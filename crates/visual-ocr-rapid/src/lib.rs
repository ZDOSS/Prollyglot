//! RapidOCR/PP-OCRv6 adapter for transient Prollyglot visual frames.
//!
//! The adapter accepts in-memory BGRA frames and returns only recognized text,
//! confidence, and capture-space geometry. It never writes source frames to
//! disk and never includes pixels in an error or diagnostic value.

mod regions;

use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
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

#[derive(Clone, Debug)]
pub struct OverlayText {
    pub text: String,
    pub bounds: VisualRect,
}

impl OverlayText {
    pub fn matches(&self, observation: &OcrObservation) -> bool {
        let bounds = observation.bounds;
        let overlap_width = (bounds.x + bounds.width).min(self.bounds.x + self.bounds.width)
            - bounds.x.max(self.bounds.x);
        let overlap_height = (bounds.y + bounds.height).min(self.bounds.y + self.bounds.height)
            - bounds.y.max(self.bounds.y);
        let covered = overlap_width.max(0.0) * overlap_height.max(0.0) / bounds.area().max(1.0);
        if covered < 0.7 {
            return false;
        }
        let normalize = |text: &str| {
            text.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .take(500)
                .collect::<String>()
        };
        let candidate = normalize(&observation.text);
        let echo = normalize(&self.text);
        !candidate.is_empty()
            && (candidate == echo || (candidate.chars().count() >= 4 && echo.contains(&candidate)))
    }
}

pub struct RapidOcrEngine {
    runner: RapidOcr,
    language_hint: String,
    profile: RecognitionProfile,
    cancellation: RapidOcrCancellation,
    timings: rapidocr_core::types::OcrTimings,
    scanner: regions::RegionScanner,
    scan_stats: OcrScanStats,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct OcrScanStats {
    pub areas_scanned: usize,
    pub areas_pending: usize,
    pub reused_lines: usize,
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
        let mut runner = RapidOcr::new(config).map_err(|error| {
            OcrError::Unavailable(format!("PP-OCRv6 Small could not load: {error:#}"))
        })?;
        runner.set_live_crop_policy(
            true,
            match profile {
                RecognitionProfile::Focused => 24,
                RecognitionProfile::AllText => 48,
            },
        );
        Ok(Self {
            runner,
            language_hint: language_hint.into(),
            profile,
            cancellation: RapidOcrCancellation::default(),
            timings: rapidocr_core::types::OcrTimings::default(),
            scanner: regions::RegionScanner::default(),
            scan_stats: OcrScanStats::default(),
        })
    }

    pub fn cancellation(&self) -> RapidOcrCancellation {
        self.cancellation.clone()
    }

    pub fn last_timings(&self) -> &rapidocr_core::types::OcrTimings {
        &self.timings
    }

    pub fn last_scan_stats(&self) -> OcrScanStats {
        self.scan_stats
    }

    /// Drop transient source pixels and pending detection when the source or
    /// its tracking generation is invalidated. Model sessions remain loaded.
    pub fn reset_scan(&mut self) {
        self.scanner.reset();
    }
}

impl OcrEngine for RapidOcrEngine {
    fn recognize(&mut self, frame: &VisualFrame) -> Result<Vec<OcrObservation>, OcrError> {
        self.recognize_filtered(frame, |_| true)
    }

    fn has_pending_work(&self) -> bool {
        self.scanner.pending()
    }

    fn reset(&mut self) {
        self.reset_scan();
    }
}

impl RapidOcrEngine {
    /// Filter raw lines before spatial grouping so an overlay cannot swallow
    /// nearby source text when the two are merged into one observation.
    pub fn recognize_filtered(
        &mut self,
        frame: &VisualFrame,
        keep: impl Fn(&OcrObservation) -> bool,
    ) -> Result<Vec<OcrObservation>, OcrError> {
        let cancellation = OcrCancellationToken::new();
        self.cancellation.begin(cancellation.clone());
        let result = self.recognize_cancellable(frame, &cancellation, keep);
        self.cancellation.finish();
        result
    }
}

impl RapidOcrEngine {
    fn recognize_cancellable(
        &mut self,
        frame: &VisualFrame,
        cancellation: &OcrCancellationToken,
        keep: impl Fn(&OcrObservation) -> bool,
    ) -> Result<Vec<OcrObservation>, OcrError> {
        cancellation.checkpoint().map_err(|_| OcrError::Cancelled)?;
        let started = Instant::now();
        self.timings = rapidocr_core::types::OcrTimings::default();
        self.scan_stats = OcrScanStats::default();
        self.scanner.prepare(frame, cancellation)?;
        self.scan_stats.reused_lines = self.scanner.observations().len();
        let mut pixels = 0;
        for (index, bounds) in self.scanner.work() {
            // One inference is indivisible, so this is a soft wall-time limit.
            // Every area has a hard dimension bound; never queue old frames.
            if pixels > 0
                && (pixels + regions::area(bounds) > regions::PIXEL_BUDGET
                    || started.elapsed() >= Duration::from_millis(450))
            {
                break;
            }
            cancellation.checkpoint().map_err(|_| OcrError::Cancelled)?;
            let cropped = frame
                .crop(bounds)
                .map_err(|error| OcrError::Inference(error.to_string()))?;
            let image = frame_to_rgb_cancellable(&cropped, cancellation)?;
            let output = self
                .runner
                .run_image_cancellable_timed(&image, cancellation)
                .map_err(|error| {
                    if is_cancelled_error(&error) || cancellation.is_cancelled() {
                        OcrError::Cancelled
                    } else {
                        OcrError::Inference(format!("PP-OCRv6 inference failed: {error:#}"))
                    }
                })?;
            self.timings.add_assign(&output.timings);
            let observations = output
                .output
                .lines
                .into_iter()
                .filter_map(|line| {
                    let mut line_bounds = quad_bounds(&line.bbox)?;
                    line_bounds.x += bounds.x as f32;
                    line_bounds.y += bounds.y as f32;
                    Some(OcrObservation {
                        text: line.text,
                        confidence: line.score,
                        language: Some(self.language_hint.clone()),
                        script: None,
                        bounds: line_bounds,
                    })
                })
                .collect();
            self.scanner.complete(index, observations);
            self.scan_stats.areas_scanned += 1;
            pixels += regions::area(bounds);
        }
        self.scan_stats.areas_pending = self.scanner.work().len();
        self.timings.total_ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(prepare_observations(
            self.scanner
                .observations()
                .into_iter()
                .filter(keep)
                .collect(),
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
        .map(|lines| {
            let lines = reading_order(lines);
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

fn reading_order(mut lines: Vec<OcrObservation>) -> Vec<OcrObservation> {
    lines.sort_by(|left, right| left.bounds.y.total_cmp(&right.bounds.y));
    let mut rows: Vec<(VisualRect, Vec<OcrObservation>)> = Vec::new();
    for line in lines {
        if let Some((_, row)) = rows.iter_mut().find(|(anchor, _)| {
            let overlap = (anchor.y + anchor.height).min(line.bounds.y + line.bounds.height)
                - anchor.y.max(line.bounds.y);
            overlap / anchor.height.min(line.bounds.height).max(1.0) >= 0.58
        }) {
            row.push(line);
        } else {
            rows.push((line.bounds, vec![line]));
        }
    }
    rows.into_iter()
        .flat_map(|(_, mut row)| {
            row.sort_by(|left, right| left.bounds.x.total_cmp(&right.bounds.x));
            row
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
    // A large false-positive background box must not absorb a nearby subtitle
    // and borrow its language/confidence. Different text sizes form separate
    // labels, as headings and body text normally should.
    maximum_height <= minimum_height * 2.0 && (same_line || stacked_lines)
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
    // Short words (No, Sí, OK) and single Han characters carry real meaning.
    // Demand stronger confidence for them, plus the same script/size evidence.
    if non_whitespace == 0
        || (non_whitespace < 3 && confidence < 0.85)
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
        // A 24-pixel subtitle is still readable source text on a 4K display.
        // Whole-display dimensions must not undo resolution-preserving OCR.
        let minimum_height = (frame_height as f32 * 0.012).clamp(16.0, 24.0);
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

    fn observation(text: &str, x: f32, y: f32) -> OcrObservation {
        OcrObservation {
            text: text.into(),
            confidence: 0.99,
            language: Some("es".into()),
            script: None,
            bounds: VisualRect {
                x,
                y,
                width: 100.0,
                height: 32.0,
            },
        }
    }

    #[test]
    fn short_spanish_words_and_single_han_characters_need_strong_evidence() {
        for (language, text) in [("es", "No"), ("es", "Sí"), ("es", "OK"), ("zh", "猫")] {
            for profile in [RecognitionProfile::Focused, RecognitionProfile::AllText] {
                assert!(
                    observation_is_useful(
                        text,
                        0.99,
                        observation(text, 0.0, 0.0).bounds,
                        1920,
                        1080,
                        language,
                        profile
                    ),
                    "{text}"
                );
                assert!(!observation_is_useful(
                    text,
                    0.60,
                    observation(text, 0.0, 0.0).bounds,
                    1920,
                    1080,
                    language,
                    profile
                ));
            }
        }
    }

    #[test]
    fn reading_order_tolerates_jitter_without_reversing_spanish_words() {
        let merged = merge_nearby_lines(vec![
            observation("Buenos", 100.0, 100.0),
            observation("días", 210.0, 99.0),
        ]);
        assert_eq!(merged[0].text, "Buenos días");
    }

    #[test]
    fn background_box_cannot_absorb_a_small_4k_subtitle() {
        let mut noise = observation("0", 1760.0, 928.0);
        noise.bounds.width = 1279.0;
        noise.bounds.height = 864.0;
        let subtitle = observation("你好世界。欢迎回来。", 1800.0, 2030.0);
        for profile in [RecognitionProfile::Focused, RecognitionProfile::AllText] {
            let result = prepare_observations(
                vec![noise.clone(), subtitle.clone()],
                3840,
                2160,
                "zh",
                profile,
            );
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].text, subtitle.text);
            assert_eq!(result[0].bounds, subtitle.bounds);
        }
    }

    #[test]
    fn overlay_filtering_preserves_adjacent_source_and_identical_text_elsewhere() {
        let echo = OverlayText {
            text: "Good morning".into(),
            bounds: VisualRect {
                x: 100.0,
                y: 160.0,
                width: 220.0,
                height: 33.0,
            },
        };
        let observations = vec![
            observation("Good morning", 100.0, 160.0),
            observation("Buenos días", 100.0, 200.0),
            observation("Good morning", 500.0, 500.0),
        ];
        let merged = merge_nearby_lines(
            observations
                .into_iter()
                .filter(|line| !echo.matches(line))
                .collect(),
        );
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|line| line.text == "Buenos días"));
        assert!(merged.iter().any(|line| line.text == "Good morning"));
    }

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
