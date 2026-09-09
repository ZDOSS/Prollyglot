#![cfg(target_os = "linux")]

#[allow(dead_code)]
mod support;

use std::{path::PathBuf, process::Command, sync::atomic::Ordering};
use support::{Behavior, MockPortal, source::VideoSource};

#[test]
#[ignore = "requires the private desktop runner, Xvfb, tauri-driver, OCR models and synthetic images"]
fn native_desktop_ocr_reader_and_picker_cancellation() {
    let root = support::private_session();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixtures = PathBuf::from(std::env::var_os("PROLLYGLOT_VISUAL_FIXTURE_DIR").unwrap());
    assert!(std::env::var_os("PROLLYGLOT_DESKTOP_TEST_BINARY").is_some());
    for (language, mode, behavior) in [
        ("zh", "portalWindow", Behavior::Normal),
        ("es", "portalDisplay", Behavior::Normal),
        ("zh", "anchorRegion", Behavior::Normal),
        ("zh", "anchorRegion2x", Behavior::Normal),
        ("es", "anchorDisplay", Behavior::Normal),
        ("zh", "cancelRegion", Behavior::Normal),
        ("es", "dismissRegion", Behavior::Normal),
        ("zh", "cancel", Behavior::Stall("Start")),
        ("es", "dismiss", Behavior::Cancel),
    ] {
        let decoded = image::open(fixtures.join(format!("{language}-1920-24-region.png")))
            .unwrap()
            .into_rgba8();
        let anchored = mode.starts_with("anchor");
        let decoded = if anchored {
            let mut canvas =
                image::RgbaImage::from_pixel(1280, 900, image::Rgba([240, 240, 240, 255]));
            image::imageops::overlay(&mut canvas, &decoded, 150, 600);
            let outside = image::open(fixtures.join("es-1920-24-region.png"))
                .unwrap()
                .into_rgba8();
            image::imageops::overlay(&mut canvas, &outside, 150, 50);
            canvas
        } else {
            decoded
        };
        let (width, height) = decoded.dimensions();
        let mut pixels = decoded.into_raw();
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        let source = VideoSource::start("desktop.fixture", 71, width, height, Some(pixels), 0);
        let portal = MockPortal::start(source.node, source.serial, 6, behavior);
        if anchored {
            *portal.observed.desktop_geometry.lock().unwrap() = Some(((0, 0), (1280, 900)));
        }
        let result = Command::new("python3")
            .arg(repo.join("scripts/check-portal-desktop.py"))
            .arg(language)
            .arg(mode)
            .env("PROLLYGLOT_DESKTOP_FIXTURE_STATE", root.join(mode))
            .status()
            .unwrap();
        assert!(
            result.success(),
            "native desktop case {language}/{mode} failed"
        );
        assert!(portal.observed.sessions_closed.load(Ordering::Acquire) >= 1);
        if behavior == Behavior::Normal {
            let parent = portal.observed.parent_window.lock().unwrap();
            assert!(
                parent.starts_with("x11:") && parent.as_str() != "x11:0",
                "main window was not supplied as picker parent"
            );
        }
    }
}
