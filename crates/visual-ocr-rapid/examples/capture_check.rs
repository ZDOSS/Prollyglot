//! Native WGC verification against the explicitly opened synthetic fixture.
//! No pixels or recognized text are written. Other windows are never selected.
#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("capture_check requires native Windows and scripts/visual-fixture.ps1");
    std::process::exit(1);
}

#[cfg(target_os = "windows")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use prollyglot_visual_ocr_rapid::RapidOcrEngine;
    use prollyglot_visual_pipeline::{OcrEngine, PixelRect};
    use prollyglot_visual_windows::{VisualCaptureSelection, source_snapshot, start_capture};
    use std::time::{Duration, Instant};

    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: capture_check MODEL_DIRECTORY zh|es".into());
    }
    let expected = match args[1].as_str() {
        "zh" => "你好世界欢迎回来",
        "es" => "buenosdíascómoestás",
        _ => return Err("fixture language must be zh or es".into()),
    };
    let waiting = Instant::now();
    let sources = loop {
        let sources = source_snapshot()?;
        if sources
            .windows
            .iter()
            .any(|window| window.label == "Prollyglot native visual fixture")
        {
            break sources;
        }
        if waiting.elapsed() >= Duration::from_secs(10) {
            return Err("fixture window did not open within ten seconds".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let window = sources
        .windows
        .iter()
        .find(|w| w.label == "Prollyglot native visual fixture")
        .ok_or("open scripts/visual-fixture.ps1 first")?;
    let display = sources
        .displays
        .iter()
        .find(|d| {
            d.x == window.x
                && d.y == window.y
                && d.width == window.width
                && d.height == window.height
                && d.width >= 980
                && d.height >= 200
        })
        .ok_or("fixture must cover one display of at least 980 by 200 pixels")?;
    let mut engine = RapidOcrEngine::load(&args[0], &args[1])?;
    for cycle in 0..3 {
        for (mode, selection) in [
            (
                "window",
                VisualCaptureSelection::ApplicationWindow {
                    source_id: window.id.clone(),
                },
            ),
            (
                "display",
                VisualCaptureSelection::Display {
                    source_id: display.id.clone(),
                },
            ),
            (
                "region",
                VisualCaptureSelection::Region {
                    display_id: display.id.clone(),
                    region: PixelRect {
                        x: display.width / 2 - 490,
                        y: display.height - 200,
                        width: 980,
                        height: 180,
                    },
                },
            ),
        ] {
            engine.reset_scan();
            let started = Instant::now();
            let mut capture = start_capture(selection)?;
            let mut found_at = None;
            let mut frames = 0;
            let mut previous = None;
            while started.elapsed() < Duration::from_secs(8) {
                let frame = capture.frames.recv_timeout(Duration::from_secs(2))?;
                if let Some((sequence, timestamp)) = previous {
                    assert!(frame.sequence > sequence && frame.captured_at_micros >= timestamp);
                }
                previous = Some((frame.sequence, frame.captured_at_micros));
                frames += 1;
                let lines = engine.recognize(&frame)?;
                let found = lines.iter().any(|line| {
                    line.text
                        .chars()
                        .filter(|c| c.is_alphanumeric())
                        .flat_map(char::to_lowercase)
                        .collect::<String>()
                        == expected
                });
                if found {
                    if found_at.is_none() {
                        found_at = Some(started.elapsed().as_secs_f64() * 1000.0);
                    }
                    if frames >= 3 {
                        break;
                    }
                }
            }
            let stop_start = Instant::now();
            capture.stop()?;
            let stop_ms = stop_start.elapsed().as_secs_f64() * 1000.0;
            let result = serde_json::json!({ "cycle": cycle, "mode": mode, "language": args[1],
                "width": capture.source.width, "height": capture.source.height, "frames": frames,
                "matched": found_at.is_some(), "captureToOcrMs": found_at, "stopMs": stop_ms });
            println!("{result}");
            if found_at.is_none() {
                return Err(format!("{mode}: expected fixture text was not detected").into());
            }
            if stop_ms > 2_000.0 {
                return Err(format!("{mode}: stopping capture exceeded two seconds").into());
            }
        }
    }
    Ok(())
}
