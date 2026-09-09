#![cfg(target_os = "linux")]

mod support;

use prollyglot_visual_pipewire::{
    CaptureEvent, CaptureOptions, CaptureSession, PortalSource, start_capture,
};
use std::{
    sync::atomic::Ordering,
    thread,
    time::{Duration, Instant},
};
use support::{Behavior, MockPortal, source::VideoSource};

fn capture(source: PortalSource) -> CaptureSession {
    start_capture(CaptureOptions {
        source,
        parent_window: "x11:1234".into(),
    })
    .unwrap()
}

fn terminal(session: &CaptureSession) -> CaptureEvent {
    let start = Instant::now();
    loop {
        let event = session
            .events
            .recv_timeout(Duration::from_secs(6))
            .expect("capture must terminate");
        match event {
            CaptureEvent::Selected(_) | CaptureEvent::FormatChanged(_) => {}
            _ => return event,
        }
        assert!(start.elapsed() < Duration::from_secs(7));
    }
}

#[test]
#[ignore = "requires the private, headless DBus/PipeWire runner"]
fn native_frames_are_selected_latest_only_and_stop_releases_sharing() {
    let source = VideoSource::start("screen.selected", 41, 320, 180, None, 0);
    let _unrelated = VideoSource::start("screen.unrelated", 205, 320, 180, None, 0);
    // v6 must select by serial even if the legacy tuple ID identifies another
    // source. The same test exercises v4's node-ID-only compatibility path.
    for version in [6, 4] {
        let portal = MockPortal::start(
            if version == 6 {
                _unrelated.node
            } else {
                source.node
            },
            source.serial,
            version,
            Behavior::Normal,
        );
        let mut session = capture(PortalSource::Monitor);
        let selected = session.events.recv_timeout(Duration::from_secs(3)).unwrap();
        let CaptureEvent::Selected(info) = selected else {
            panic!("{selected:?}");
        };
        assert_eq!(info.logical_position, Some((-1600, 120)));
        assert_eq!(info.logical_size, Some((1600, 900)));
        let frame = session
            .frames
            .recv_timeout(Duration::from_secs(6))
            .unwrap_or_else(|e| panic!("{e}: {:?}", session.events.try_iter().collect::<Vec<_>>()));
        assert_eq!((frame.width, frame.height), (320, 180));
        assert_eq!(
            frame.pixels()[0],
            41,
            "unrelated pixels must never enter OCR"
        );
        assert_eq!(frame.pixels()[3], 255);
        thread::sleep(Duration::from_millis(450));
        assert_eq!(session.frames.len(), 1);
        let latest = session.frames.recv().unwrap();
        assert!(latest.sequence > frame.sequence + 2);
        assert!(latest.captured_at_micros > frame.captured_at_micros);
        let produced = source.frames_sent.load(Ordering::Acquire) as u8;
        assert!(
            produced.wrapping_sub(latest.pixels()[1]) <= 1,
            "stale native buffer"
        );
        let started = Instant::now();
        session.stop().unwrap();
        session.stop().unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(session.frames.is_empty());
        assert!(portal.observed.sessions_closed.load(Ordering::Acquire) >= 1);
        let selection = portal.observed.selection.lock().unwrap();
        let selection = selection.as_ref().unwrap();
        assert!(!bool::try_from(selection.get("multiple").unwrap()).unwrap());
        assert_eq!(
            u32::try_from(selection.get("persist_mode").unwrap()).unwrap(),
            0
        );
        assert_eq!(
            u32::try_from(selection.get("cursor_mode").unwrap()).unwrap(),
            1
        );
        assert!(!selection.contains_key("restore_token"));
    }
}

#[test]
#[ignore = "requires the private, headless DBus/PipeWire runner"]
fn startup_cancellation_and_portal_failures_release_requests_and_sessions() {
    support::private_session();
    for (method, behavior) in ["CreateSession", "SelectSources", "Start"]
        .into_iter()
        .flat_map(|method| {
            [
                (method, Behavior::Stall(method)),
                (method, Behavior::StallReply(method)),
            ]
        })
    {
        let portal = MockPortal::start(42, 200, 6, behavior);
        let mut session = capture(PortalSource::Window);
        portal.wait_method(method);
        let start = Instant::now();
        session.stop().unwrap();
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(terminal(&session), CaptureEvent::Cancelled);
        assert!(
            portal.observed.requests_closed.load(Ordering::Acquire) >= 1,
            "{method}"
        );
        assert!(
            portal.observed.sessions_closed.load(Ordering::Acquire) >= 1,
            "{method}"
        );
        assert!(!portal.observed.remote_opened.load(Ordering::Acquire));
    }
    for behavior in [
        Behavior::Cancel,
        Behavior::Reject,
        Behavior::Revoke,
        Behavior::MissingSerial,
        Behavior::WrongSource,
        Behavior::Multiple,
    ] {
        let portal = MockPortal::start(42, 200, 6, behavior);
        let mut session = capture(PortalSource::Window);
        let event = terminal(&session);
        match behavior {
            Behavior::Cancel => assert_eq!(event, CaptureEvent::Cancelled),
            Behavior::Revoke => assert_eq!(event, CaptureEvent::Closed),
            _ => assert!(
                matches!(event, CaptureEvent::Failed(_)),
                "{behavior:?}: {event:?}"
            ),
        }
        session.stop().unwrap();
        assert!(portal.observed.sessions_closed.load(Ordering::Acquire) >= 1);
        assert!(!portal.observed.remote_opened.load(Ordering::Acquire));
    }
    let portal = MockPortal::start(42, 200, 6, Behavior::Stall("Start"));
    let mut session = capture(PortalSource::Window);
    portal.wait_method("Start");
    drop(portal);
    assert!(matches!(terminal(&session), CaptureEvent::Failed(_)));
    session.stop().unwrap();
}

#[test]
#[ignore = "requires the private, headless DBus/PipeWire runner"]
fn resolution_changes_revocation_and_source_loss_do_not_switch_sources() {
    let source = VideoSource::start("screen.resizable", 19, 320, 180, None, 15);
    let portal = MockPortal::start(source.node, source.serial, 6, Behavior::Normal);
    let mut session = capture(PortalSource::Window);
    let CaptureEvent::Selected(info) = session.events.recv_timeout(Duration::from_secs(3)).unwrap()
    else {
        panic!("selection");
    };
    assert_eq!(
        info.logical_position, None,
        "window position is not portable"
    );
    session.frames.recv_timeout(Duration::from_secs(6)).unwrap();
    source.resize(640, 360);
    let start = Instant::now();
    loop {
        let frame = session.frames.recv_timeout(Duration::from_secs(6)).unwrap();
        if (frame.width, frame.height) == (640, 360) {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "format renegotiation"
        );
    }
    portal.revoke();
    assert_eq!(terminal(&session), CaptureEvent::Closed);
    session.stop().unwrap();
    drop(portal);
    let portal = MockPortal::start(source.node, source.serial, 6, Behavior::Normal);
    let mut session = capture(PortalSource::Monitor);
    session.frames.recv_timeout(Duration::from_secs(6)).unwrap();
    drop(source);
    let _replacement = VideoSource::start("screen.resizable", 211, 320, 180, None, 0);
    assert_eq!(terminal(&session), CaptureEvent::Closed);
    session.stop().unwrap();
    assert!(session.frames.is_empty());
    assert!(portal.observed.sessions_closed.load(Ordering::Acquire) >= 1);
}

#[test]
#[ignore = "requires private services, installed OCR models and synthetic subtitle fixtures"]
fn native_ocr_recognizes_chinese_and_spanish_from_portal_frames() {
    use prollyglot_visual_ocr_rapid::RapidOcrEngine;
    use prollyglot_visual_pipeline::OcrEngine;
    support::private_session();
    let models =
        std::env::var_os("PROLLYGLOT_VISUAL_OCR_MODEL_DIR").expect("provide installed OCR models");
    let fixtures = std::path::PathBuf::from(
        std::env::var_os("PROLLYGLOT_VISUAL_FIXTURE_DIR")
            .expect("provide synthetic fixtures from scripts/check-visual-browser.mjs"),
    );
    let normalize = |text: &str| {
        text.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    for (language, expected) in [
        ("zh", "你好世界。欢迎回来。"),
        ("es", "Buenos días. ¿Cómo estás?"),
    ] {
        let mut engine = RapidOcrEngine::load(&models, language).unwrap();
        for suffix in ["1920-24", "3840-24", "1920-24-region"] {
            let image = image::open(fixtures.join(format!("{language}-{suffix}.png")))
                .unwrap()
                .into_rgba8();
            let (width, height) = image.dimensions();
            let mut pixels = image.into_raw();
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            let source = VideoSource::start("screen.ocr", 0, width, height, Some(pixels), 0);
            let _portal = MockPortal::start(source.node, source.serial, 6, Behavior::Normal);
            let start = Instant::now();
            let mut session = capture(PortalSource::Monitor);
            let frame = session.frames.recv_timeout(Duration::from_secs(6)).unwrap();
            let capture_ms = start.elapsed().as_millis();
            engine.reset_scan();
            let ocr_started = Instant::now();
            let mut recognized = String::new();
            for _ in 0..24 {
                let observations = engine.recognize(&frame).unwrap();
                recognized = observations
                    .iter()
                    .map(|o| o.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                if normalize(&recognized).contains(&normalize(expected))
                    || !engine.has_pending_work()
                {
                    break;
                }
            }
            assert!(
                normalize(&recognized).contains(&normalize(expected)),
                "{language}-{suffix}: {recognized}"
            );
            println!(
                "portal OCR {language}-{suffix}: first frame {capture_ms} ms, recognition {} ms",
                ocr_started.elapsed().as_millis()
            );
            session.stop().unwrap();
        }
    }
}
