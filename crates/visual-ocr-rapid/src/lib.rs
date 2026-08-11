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
}

impl RapidOcrEngine {
    pub fn load(
        model_directory: impl AsRef<Path>,
        language_hint: impl Into<String>,
    ) -> Result<Self, OcrError> {
        let config = RapidOcrConfig::ppocr_v6_small(model_directory.as_ref());
        let runner = RapidOcr::new(config).map_err(|error| {
            OcrError::Unavailable(format!("PP-OCRv6 Small could not load: {error:#}"))
        })?;
        Ok(Self {
            runner,
            language_hint: language_hint.into(),
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
                Some(OcrObservation {
                    text: line.text,
                    confidence: line.score,
                    language: Some(self.language_hint.clone()),
                    script: None,
                    bounds,
                })
            })
            .collect())
    }
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
    #[ignore = "requires a downloaded PP-OCRv6 model directory"]
    fn loads_verified_ppocrv6_artifacts() {
        let directory = std::env::var_os("PROLLYGLOT_VISUAL_OCR_MODEL_DIR")
            .expect("set PROLLYGLOT_VISUAL_OCR_MODEL_DIR");
        RapidOcrEngine::load(directory, "ja").expect("PP-OCRv6 model should initialize");
    }
}
