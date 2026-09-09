//! Pixel evidence for feedback filtering in the unanchored Ubuntu reader.
//! Matching words alone must not suppress identical text elsewhere in a video.
//! The opaque reader background is checked in the current captured frame, so
//! this does not depend on compositor window positions or display scaling.

use prollyglot_visual_pipeline::{VisualFrame, VisualRect};

pub fn has_reader_background(frame: &VisualFrame, bounds: VisualRect) -> bool {
    if !bounds.is_valid()
        || bounds.x + bounds.width > frame.width as f32
        || bounds.y + bounds.height > frame.height as f32
    {
        return false;
    }
    // Keep in sync with .visual-reader-body in visual-overlay.css. A small
    // tolerance permits rounding in compositor color conversion. This is a
    // conservative heuristic, not an exclusion guarantee for every compositor.
    let background = [0x39_u8, 0x2d, 0x13]; // BGRA: CSS #132d39
    let mut matching = 0;
    for row in 0..7 {
        for column in 0..7 {
            let x = (bounds.x + bounds.width * (column as f32 + 0.5) / 7.0) as usize;
            let y = (bounds.y + bounds.height * (row as f32 + 0.5) / 7.0) as usize;
            let offset = y * frame.stride + x * 4;
            if frame.pixels().get(offset..offset + 3).is_some_and(|pixel| {
                pixel
                    .iter()
                    .zip(background)
                    .all(|(actual, expected)| actual.abs_diff(expected) <= 8)
            }) {
                matching += 1;
            }
        }
    }
    // Leave room for antialiased glyphs without masking ordinary black/white
    // subtitles that happen to contain the same translated words.
    matching >= 30
}

#[cfg(test)]
mod tests {
    use super::*;
    use prollyglot_visual_ocr_rapid::OverlayText;
    use prollyglot_visual_pipeline::{OcrObservation, PixelFormat};

    #[test]
    fn reader_feedback_requires_both_rendered_text_and_local_pixel_evidence() {
        let mut pixels = vec![255; 200 * 80 * 4];
        for y in 0..80 {
            for x in 100..200 {
                let offset = (y * 200 + x) * 4;
                pixels[offset..offset + 4].copy_from_slice(&[0x39, 0x2d, 0x13, 255]);
            }
        }
        let frame = VisualFrame::new(1, 0, 200, 80, 800, PixelFormat::Bgra8, pixels).unwrap();
        let echo = OverlayText {
            text: "Good morning".into(),
            bounds: VisualRect {
                x: 0.0,
                y: 0.0,
                width: 200.0,
                height: 80.0,
            },
        };
        let mut observation = OcrObservation {
            text: "Good morning".into(),
            confidence: 0.95,
            language: Some("en".into()),
            script: None,
            bounds: VisualRect {
                x: 10.0,
                y: 10.0,
                width: 80.0,
                height: 30.0,
            },
        };
        let matches = |observation: &OcrObservation| {
            echo.matches(observation) && has_reader_background(&frame, observation.bounds)
        };
        assert!(
            !matches(&observation),
            "same words in source must remain eligible"
        );
        observation.bounds.x = 110.0;
        assert!(
            matches(&observation),
            "reader translation should be filtered"
        );
        observation.text = "Different source words".into();
        assert!(
            !matches(&observation),
            "background alone must never mask source text"
        );
        observation.bounds.x = f32::NAN;
        assert!(!has_reader_background(&frame, observation.bounds));
        observation.bounds.x = 190.0;
        assert!(!has_reader_background(&frame, observation.bounds));
        assert!(
            include_str!("../../src/styles/visual-overlay.css").contains("background: #132d39;")
        );
    }
}
