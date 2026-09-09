#![cfg(target_os = "linux")]

mod support;

use crossbeam_channel::Receiver;
use prollyglot_audio_pipewire::PipeWireAudioCaptureBackend;
use prollyglot_core::{AudioCaptureBackend, CaptureEvent, CaptureSelection, SourceId};
use std::{
    io::Write,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn command(program: &str, args: &[&str]) -> String {
    let output = Command::new(program).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{program}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn sink(name: &str) {
    command(
        "pw-cli",
        &[
            "create-node",
            "adapter",
            &format!(
                "{{ factory.name = support.null-audio-sink node.name = {name} node.description = {name} media.class = Audio/Sink object.linger = true audio.position = [ FL FR ] }}"
            ),
        ],
    );
}

fn node_id(name: &str) -> String {
    let graph: serde_json::Value = serde_json::from_str(&command("pw-dump", &[])).unwrap();
    graph
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["info"]["props"]["node.name"] == name)
        .unwrap()["id"]
        .to_string()
}

fn default(name: &str, backend: &PipeWireAudioCaptureBackend) {
    command(
        "pw-metadata",
        &[
            "-n",
            "default",
            "0",
            "default.configured.audio.sink",
            &format!("{{\"name\":\"{name}\"}}"),
            "Spa:String:JSON",
        ],
    );
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if backend
            .source_snapshot()
            .unwrap()
            .playback_devices
            .iter()
            .any(|d| d.name == name && d.is_default)
        {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("default metadata did not select {name}");
}

struct Tone(Child);
impl Tone {
    fn start(target: &str, frequency: f64) -> Self {
        Self::with_application(target, frequency, "")
    }

    fn with_application(target: &str, frequency: f64, application: &str) -> Self {
        let mut child = Command::new("pw-cat")
            .args([
                "--playback",
                "--raw",
                "--format",
                "f32",
                "--rate",
                "48000",
                "--channels",
                "1",
                "--target",
                target,
                "-P",
                &format!("{{ node.dont-move = true node.dont-fallback = true node.dont-reconnect = true {application} }}"),
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        thread::spawn(move || {
            let bytes: Vec<_> = (0..48000)
                .flat_map(|i| {
                    ((std::f64::consts::TAU * frequency * f64::from(i) / 48000.0).sin() as f32
                        * 0.25)
                        .to_le_bytes()
                })
                .collect();
            for _ in 0..60 {
                if input.write_all(&bytes).is_err() {
                    break;
                }
            }
        });
        Self(child)
    }
}
impl Drop for Tone {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn probe(
    events: &Receiver<CaptureEvent>,
    expected: f64,
    rejected: f64,
    previous: Option<(u64, u64)>,
    new_stream: bool,
) -> (u64, u64) {
    let started = Instant::now();
    let mut samples = Vec::new();
    let mut last = previous;
    let mut waiting = new_stream;
    let mut rate = 0;
    while started.elapsed() < Duration::from_secs(8) {
        match events.recv_timeout(Duration::from_millis(100)) {
            Ok(CaptureEvent::Frame(frame)) => {
                if let Some((sequence, time)) = last {
                    assert!(frame.sequence > sequence && frame.captured_at_micros >= time);
                }
                last = Some((frame.sequence, frame.captured_at_micros));
                if waiting && !frame.discontinuity {
                    continue;
                }
                waiting = false;
                rate = frame.sample_rate;
                samples.extend(frame.samples);
                if samples.len() >= rate as usize {
                    break;
                }
            }
            Ok(CaptureEvent::Error(e)) => panic!("capture error: {e}"),
            Ok(CaptureEvent::Recovery(e)) => println!("recovery: {}", e.message),
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                panic!("capture channel closed")
            }
            _ => {}
        }
    }
    assert!(rate > 0 && samples.len() >= rate as usize, "not enough PCM");
    let amplitude = |frequency: f64| {
        let (mut real, mut imag) = (0.0_f64, 0.0_f64);
        for (i, sample) in samples.iter().enumerate() {
            let phase = std::f64::consts::TAU * frequency * i as f64 / f64::from(rate);
            real += f64::from(*sample) * phase.cos();
            imag += f64::from(*sample) * phase.sin();
        }
        2.0 * real.hypot(imag) / samples.len() as f64
    };
    let selected = amplitude(expected);
    let other = amplitude(rejected);
    println!(
        "{}",
        serde_json::json!({"expectedHz":expected,"selectedAmplitude":selected,"otherAmplitude":other,"sampleRate":rate})
    );
    assert!(
        selected > 0.05 && other < selected * 0.01,
        "wrong output/silence"
    );
    last.unwrap()
}

fn recovery(events: &Receiver<CaptureEvent>) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if matches!(
            events.recv_timeout(Duration::from_millis(100)),
            Ok(CaptureEvent::Recovery(_))
        ) {
            return;
        }
    }
    panic!("unavailable source did not publish recovery");
}

#[test]
#[ignore = "run scripts/check-pipewire.py to create an isolated graph"]
fn private_graph_routing_recreation_and_stop() {
    require_private_graph();
    let backend = PipeWireAudioCaptureBackend::new();
    sink("prollyglot-test-a");
    sink("prollyglot-test-b");
    default("prollyglot-test-a", &backend);
    let a_id = backend
        .source_snapshot()
        .unwrap()
        .playback_devices
        .into_iter()
        .find(|d| d.name == "prollyglot-test-a")
        .unwrap()
        .id;
    assert!(
        backend
            .resolve_selection(&CaptureSelection::Application {
                source_id: SourceId::new("invalid")
            })
            .is_err()
    );
    let mut a = Some(Tone::start("prollyglot-test-a", 997.0));
    let _b = Tone::start("prollyglot-test-b", 1733.0);
    thread::sleep(Duration::from_millis(300));
    let (default_tx, default_rx) = crossbeam_channel::bounded(1024);
    let (pinned_tx, pinned_rx) = crossbeam_channel::bounded(1024);
    let mut follow = backend
        .start_capture(CaptureSelection::SystemDefault, default_tx)
        .unwrap();
    let mut pinned = backend
        .start_capture(
            CaptureSelection::SystemOutput {
                device_id: a_id.clone(),
            },
            pinned_tx,
        )
        .unwrap();
    let followed = probe(&default_rx, 997.0, 1733.0, None, true);
    let pinned_last = probe(&pinned_rx, 997.0, 1733.0, None, true);
    default_rx.try_iter().for_each(drop);
    pinned_rx.try_iter().for_each(drop);
    default("prollyglot-test-b", &backend);
    probe(&default_rx, 1733.0, 997.0, Some(followed), true);
    probe(&pinned_rx, 997.0, 1733.0, Some(pinned_last), false);
    drop(a.take());
    command("pw-cli", &["destroy", &node_id("prollyglot-test-a")]);
    recovery(&pinned_rx);
    let wait = Instant::now();
    while wait.elapsed() < Duration::from_millis(500) {
        assert!(
            !matches!(
                pinned_rx.recv_timeout(Duration::from_millis(50)),
                Ok(CaptureEvent::Frame(_))
            ),
            "pinned capture fell back after removal"
        );
    }
    sink("prollyglot-test-a");
    a = Some(Tone::start("prollyglot-test-a", 997.0));
    assert!(
        backend
            .source_snapshot()
            .unwrap()
            .playback_devices
            .iter()
            .any(|d| d.id == a_id)
    );
    probe(&pinned_rx, 997.0, 1733.0, Some(pinned_last), true);
    for session in [&mut follow, &mut pinned] {
        let started = Instant::now();
        session.stop().unwrap();
        println!(
            "{}",
            serde_json::json!({"stopMs":started.elapsed().as_secs_f64()*1000.0})
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }
    drop(a);
}

fn require_private_graph() {
    let runtime =
        std::env::var("PROLLYGLOT_PRIVATE_PIPEWIRE").expect("private graph runner required");
    assert_eq!(std::env::var("PIPEWIRE_RUNTIME_DIR").unwrap(), runtime);
    assert_eq!(std::env::var("PIPEWIRE_REMOTE").unwrap(), "pipewire-0");
    assert!(
        std::path::Path::new(&runtime)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("prollyglot-pipewire-")
    );
}

fn application_id(backend: &PipeWireAudioCaptureBackend, name: &str) -> SourceId {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        if let Some(app) = backend
            .source_snapshot()
            .unwrap()
            .applications
            .into_iter()
            .find(|app| app.name == name)
        {
            assert_eq!(app.instance_count, 1);
            return app.id;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("application {name} was not enumerated");
}

fn assert_playback_routes(routes: &[(&str, &str)], monitor_present: bool) {
    let graph: serde_json::Value = serde_json::from_str(&command("pw-dump", &[])).unwrap();
    let objects = graph.as_array().unwrap();
    let node = |name: &str| {
        objects
            .iter()
            .find(|object| object["info"]["props"]["node.name"] == name)
            .map(|object| object["id"].clone())
    };
    assert_eq!(
        node("prollyglot-application-captions").is_some(),
        monitor_present
    );
    for (source, target) in routes {
        let source_id = node(source).unwrap();
        let target_id = node(target).unwrap();
        assert!(
            objects.iter().any(|object| {
                object["type"] == "PipeWire:Interface:Link"
                    && object["info"]["output-node-id"] == source_id
                    && object["info"]["input-node-id"] == target_id
                    && object["info"]["state"] == "active"
            }),
            "normal playback route was removed: {source} to {target}"
        );
    }
}

fn mixed_probe(
    events: &Receiver<CaptureEvent>,
    expected: &[f64],
    rejected: &[f64],
    previous: Option<(u64, u64)>,
    discontinuity: bool,
) -> (u64, u64) {
    let started = Instant::now();
    let mut samples = Vec::new();
    let mut first_time = None;
    let mut last = previous;
    let mut waiting = discontinuity;
    while started.elapsed() < Duration::from_secs(6) {
        if let Ok(CaptureEvent::Frame(frame)) = events.recv_timeout(Duration::from_millis(100)) {
            if let Some((sequence, time)) = last {
                assert!(frame.sequence > sequence && frame.captured_at_micros >= time);
            }
            last = Some((frame.sequence, frame.captured_at_micros));
            if waiting && !frame.discontinuity {
                continue;
            }
            waiting = false;
            assert_eq!(frame.sample_rate, 48000);
            first_time.get_or_insert(frame.captured_at_micros);
            samples.extend(frame.samples);
            if samples.len() >= 48000 {
                break;
            }
        }
    }
    assert!(samples.len() >= 48000, "application produced no PCM");
    let elapsed = last.unwrap().1 - first_time.unwrap();
    assert!(
        (800_000..1_300_000).contains(&elapsed),
        "mixed streams stretched/compressed audio time: {elapsed}"
    );
    let amplitude = |frequency: f64| {
        let (mut real, mut imag) = (0.0_f64, 0.0_f64);
        for (i, sample) in samples.iter().enumerate() {
            let phase = std::f64::consts::TAU * frequency * i as f64 / 48000.0;
            real += f64::from(*sample) * phase.cos();
            imag += f64::from(*sample) * phase.sin();
        }
        2.0 * real.hypot(imag) / samples.len() as f64
    };
    let selected: Vec<_> = expected
        .iter()
        .map(|frequency| amplitude(*frequency))
        .collect();
    let other: Vec<_> = rejected
        .iter()
        .map(|frequency| amplitude(*frequency))
        .collect();
    println!(
        "{}",
        serde_json::json!({"selected":selected,"other":other,"audioMicros":elapsed})
    );
    assert!(selected.iter().all(|amplitude| *amplitude > 0.08));
    assert!(other.iter().all(|amplitude| *amplitude < 0.002));
    last.unwrap()
}

#[test]
#[ignore = "run scripts/check-pipewire.py to create an isolated graph"]
fn private_application_mix_stream_recreation_and_ambiguity() {
    require_private_graph();
    let backend = PipeWireAudioCaptureBackend::new();
    let target = "prollyglot-app-output";
    let second_target = "prollyglot-app-second-output";
    let app = "org.prollyglot.fixture-a";
    sink(target);
    sink(second_target);
    let mut a = Some(support::NativeTone::start(
        app,
        "prollyglot-app-a1",
        target,
        997.0,
    ));
    let mut a2 = Some(support::NativeTone::start(
        app,
        "prollyglot-app-a2",
        second_target,
        1733.0,
    ));
    let _b = support::NativeTone::start(
        "org.prollyglot.fixture-b",
        "prollyglot-app-b",
        target,
        2333.0,
    );
    let id = application_id(&backend, app);
    thread::sleep(Duration::from_millis(300));
    let (tx, rx) = crossbeam_channel::bounded(1024);
    let mut session = backend
        .start_capture(
            CaptureSelection::Application {
                source_id: id.clone(),
            },
            tx,
        )
        .unwrap();
    let mut last = mixed_probe(&rx, &[997.0, 1733.0], &[2333.0], None, true);
    assert_playback_routes(
        &[
            ("prollyglot-app-a1", target),
            ("prollyglot-app-a2", second_target),
            ("prollyglot-app-b", target),
        ],
        true,
    );
    // Removing one stream must leave the other stream on its original clock.
    drop(a.take());
    thread::sleep(Duration::from_millis(200));
    rx.try_iter().for_each(drop);
    last = mixed_probe(&rx, &[1733.0], &[997.0, 2333.0], Some(last), false);
    a = Some(support::NativeTone::start(
        app,
        "prollyglot-app-a1-new",
        target,
        997.0,
    ));
    thread::sleep(Duration::from_millis(200));
    rx.try_iter().for_each(drop);
    last = mixed_probe(&rx, &[997.0, 1733.0], &[2333.0], Some(last), false);
    // An independent process claiming the same application identity is
    // ambiguous, even if its playback metadata has the same friendly name.
    let duplicate = Tone::with_application(
        target,
        3119.0,
        &format!("application.id = {app} application.name = {app}"),
    );
    recovery(&rx);
    assert_eq!(
        backend
            .source_snapshot()
            .unwrap()
            .applications
            .into_iter()
            .find(|app| app.id == id)
            .unwrap()
            .instance_count,
        2
    );
    assert!(
        backend
            .resolve_selection(&CaptureSelection::Application {
                source_id: id.clone()
            })
            .is_err()
    );
    assert!(!matches!(
        rx.recv_timeout(Duration::from_millis(200)),
        Ok(CaptureEvent::Frame(_))
    ));
    drop(duplicate);
    last = mixed_probe(&rx, &[997.0, 1733.0], &[2333.0, 3119.0], Some(last), true);
    drop(a.take());
    drop(a2.take());
    recovery(&rx);
    assert!(!matches!(
        rx.recv_timeout(Duration::from_millis(200)),
        Ok(CaptureEvent::Frame(_))
    ));
    a = Some(support::NativeTone::start(
        app,
        "prollyglot-app-a1-resumed",
        target,
        997.0,
    ));
    assert_eq!(application_id(&backend, app), id);
    mixed_probe(&rx, &[997.0], &[1733.0, 2333.0], Some(last), true);
    let stopped = Instant::now();
    session.stop().unwrap();
    assert!(stopped.elapsed() < Duration::from_secs(2));
    println!(
        "{}",
        serde_json::json!({"applicationStopMs":stopped.elapsed().as_secs_f64()*1000.0})
    );
    assert_playback_routes(
        &[
            ("prollyglot-app-a1-resumed", target),
            ("prollyglot-app-b", target),
        ],
        false,
    );
    drop(a);

    // A real player process may exit and return with a different PID. Keep
    // its stable app selection and resume without including the other player.
    let properties = "application.id = org.prollyglot.restart application.name = RestartPlayer node.name = prollyglot-restart-player";
    let player = Tone::with_application(target, 3571.0, properties);
    let pid = player.0.id();
    let id = application_id(&backend, "RestartPlayer");
    let (tx, rx) = crossbeam_channel::bounded(1024);
    let mut session = backend
        .start_capture(
            CaptureSelection::Application {
                source_id: id.clone(),
            },
            tx,
        )
        .unwrap();
    let last = mixed_probe(&rx, &[3571.0], &[2333.0], None, true);
    drop(player);
    recovery(&rx);
    assert!(!matches!(
        rx.recv_timeout(Duration::from_millis(200)),
        Ok(CaptureEvent::Frame(_))
    ));
    let player = Tone::with_application(target, 3571.0, properties);
    assert_ne!(player.0.id(), pid);
    assert_eq!(application_id(&backend, "RestartPlayer"), id);
    mixed_probe(&rx, &[3571.0], &[2333.0], Some(last), true);
    session.stop().unwrap();
    assert_playback_routes(
        &[
            ("prollyglot-restart-player", target),
            ("prollyglot-app-b", target),
        ],
        false,
    );
}
