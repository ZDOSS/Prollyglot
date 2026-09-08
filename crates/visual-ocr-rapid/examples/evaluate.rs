//! Explicit, local-only OCR evaluation of user-supplied PNG fixtures.
use prollyglot_visual_ocr_rapid::{RapidOcrEngine, RecognitionProfile};
use prollyglot_visual_pipeline::{OcrEngine, PixelFormat, VisualFrame, VisualRect};
use std::{error::Error, path::Path, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let profile = if args.first().is_some_and(|arg| arg == "--focused") {
        args.remove(0);
        RecognitionProfile::Focused
    } else {
        RecognitionProfile::AllText
    };
    if args.len() < 3 {
        return Err(
            "usage: evaluate [--focused] MODEL_DIRECTORY LANGUAGE IMAGE.png [IMAGE.png ...]".into(),
        );
    }
    let started = Instant::now();
    let mut engine = RapidOcrEngine::load_with_profile(&args[0], &args[1], profile)?;
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut failures = 0;
    for path in &args[2..] {
        let manifest_path = Path::new(path)
            .parent()
            .unwrap_or(Path::new("."))
            .join("manifest.json");
        let manifest: serde_json::Value = match std::fs::read(manifest_path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => serde_json::Value::Null,
            Err(error) => return Err(error.into()),
        };
        let metadata = &manifest[Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .as_ref()];
        let expected = metadata["expectedText"].as_str();
        let expected_bounds: Option<VisualRect> =
            serde_json::from_value(metadata["bounds"].clone()).ok();
        let matches = |lines: &[prollyglot_visual_pipeline::OcrObservation]| {
            expected.is_some_and(|text| lines.iter().any(|line| line.text == text))
        };
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
            // Measure actual inference on every pass, separately from cache
            // reuse. A fixture may need several bounded scans to cover it.
            engine.reset_scan();
            let started = Instant::now();
            let mut regions = engine.recognize(&frame)?;
            let mut timings = engine.last_timings().clone();
            let mut max_scan_ms = timings.total_ms;
            let first_ms = started.elapsed().as_secs_f64() * 1000.0;
            let mut first_text_ms = (!regions.is_empty()).then_some(first_ms);
            let mut first_match_ms = matches(&regions).then_some(first_ms);
            let mut scans = 1;
            while engine.has_pending_work() {
                regions = engine.recognize(&frame)?;
                timings.add_assign(engine.last_timings());
                max_scan_ms = max_scan_ms.max(engine.last_timings().total_ms);
                if first_text_ms.is_none() && !regions.is_empty() {
                    first_text_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
                }
                if first_match_ms.is_none() && matches(&regions) {
                    first_match_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
                }
                scans += 1;
                if scans > 256 {
                    return Err("fixture did not finish scanning".into());
                }
            }
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            let cache_start = Instant::now();
            let cached = engine.recognize(&frame)?;
            assert_eq!(
                regions, cached,
                "unchanged pixels must retain the same text"
            );
            let cache_ms = cache_start.elapsed().as_secs_f64() * 1000.0;
            assert!(
                engine.recognize_filtered(&frame, |_| false)?.is_empty(),
                "new overlay geometry must filter even cached OCR"
            );
            let exact_match = expected.map(|_| matches(&regions));
            let geometry_match = expected_bounds.map(|bounds| {
                regions.iter().any(|line| {
                    Some(line.text.as_str()) == expected
                        && line.bounds.intersection_over_union(bounds) >= 0.3
                })
            });
            if exact_match == Some(false) || geometry_match == Some(false) {
                failures += 1;
            }
            println!(
                "{}",
                serde_json::json!({
                    "fixture": path, "language": args[1], "width": width, "height": height,
                    "profile": format!("{profile:?}"),
                    "loadMs": load_ms, "pass": pass, "elapsedMs": elapsed_ms,
                    "firstScanMs": first_ms, "scans": scans,
                    "maxScanMs": max_scan_ms,
                    "firstTextMs": first_text_ms,
                    "expectedText": expected, "exactMatch": exact_match, "geometryMatch": geometry_match,
                    "firstMatchMs": first_match_ms,
                    "cacheMs": cache_ms,
                    "timings": timings, "regions": regions
                })
            );
        }
        let blank = VisualFrame::new(
            2,
            250_000,
            width,
            height,
            width as usize * 4,
            PixelFormat::Bgra8,
            [25, 25, 25, 255].repeat(width as usize * height as usize),
        )?;
        assert!(
            engine.recognize(&blank)?.is_empty(),
            "disappeared source text must leave the cache"
        );
    }
    if failures > 0 {
        return Err(format!("{failures} fixture passes failed text or geometry acceptance").into());
    }
    Ok(())
}
