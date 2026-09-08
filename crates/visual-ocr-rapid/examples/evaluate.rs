//! Explicit, local-only OCR evaluation of user-supplied PNG fixtures.
use prollyglot_visual_ocr_rapid::{RapidOcrEngine, RecognitionProfile};
use prollyglot_visual_pipeline::{OcrEngine, PixelFormat, VisualFrame};
use std::{error::Error, path::Path, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        return Err("usage: evaluate MODEL_DIRECTORY LANGUAGE IMAGE.png [IMAGE.png ...]".into());
    }
    let started = Instant::now();
    let mut engine =
        RapidOcrEngine::load_with_profile(&args[0], &args[1], RecognitionProfile::AllText)?;
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    for path in &args[2..] {
        let image = image::open(Path::new(path))?.to_rgba8();
        let (width, height) = image.dimensions();
        let mut pixels = image.into_raw();
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let frame = VisualFrame::new(
            1,
            0,
            width,
            height,
            width as usize * 4,
            PixelFormat::Bgra8,
            pixels,
        )?;
        for pass in 0..3 {
            let started = Instant::now();
            let regions = engine.recognize(&frame)?;
            println!(
                "{}",
                serde_json::json!({
                    "fixture": path, "language": args[1], "width": width, "height": height,
                    "loadMs": load_ms, "pass": pass, "elapsedMs": started.elapsed().as_secs_f64() * 1000.0,
                    "timings": engine.last_timings(), "regions": regions
                })
            );
        }
    }
    Ok(())
}
