//! RapidOCR/PP-OCRv6 adapter for transient Prollyglot visual frames.
//!
//! The adapter accepts in-memory BGRA frames and returns only recognized text,
//! confidence, and capture-space geometry. It never writes source frames to
//! disk and never includes pixels in an error or diagnostic value.

use std::path::Path;

use image::RgbImage;
use prollyglot_visual_pipeline::{
    OcrEngine, OcrError, OcrObservation, PixelFormat, VisualFrame, VisualRect,
};
use rapidocr_core::{RapidOcr, config::RapidOcrConfig, types::Quad};

pub struct RapidOcrEngine {
    runner: RapidOcr,
    language_hint: String,
    profile: RecognitionProfile,
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
        let config = RapidOcrConfig::ppocr_v6_small(model_directory.as_ref());
        let runner = RapidOcr::new(config).map_err(|error| {
            OcrError::Unavailable(format!("PP-OCRv6 Small could not load: {error:#}"))
        })?;
        Ok(Self {
            runner,
            language_hint: language_hint.into(),
            profile,
        })
    }
}

impl OcrEngine for RapidOcrEngine {
    fn recognize(&mut self, frame: &VisualFrame) -> Result<Vec<OcrObservation>, OcrError> {
        let image = frame_to_rgb(frame)?;
        let output = self.runner.run_image(&image).map_err(|error| {
            OcrError::Inference(format!("PP-OCRv6 inference failed: {error:#}"))
        })?;
        Ok(output
            .lines
            .into_iter()
            .filter_map(|line| {
                let bounds = quad_bounds(&line.bbox)?;
                let script = dominant_script(&line.text);
                if !observation_is_useful(
                    &line.text,
                    line.score,
                    bounds,
                    frame.width,
                    frame.height,
                    &self.language_hint,
                    self.profile,
                ) {
                    return None;
                }
                Some(OcrObservation {
                    text: line.text,
                    confidence: line.score,
                    language: Some(self.language_hint.clone()),
                    script: script.map(str::to_owned),
                    bounds,
                })
            })
            .collect())
    }
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
        let minimum_height = (frame_height as f32 * 0.018).clamp(14.0, 30.0);
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

fn frame_to_rgb(frame: &VisualFrame) -> Result<RgbImage, OcrError> {
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
