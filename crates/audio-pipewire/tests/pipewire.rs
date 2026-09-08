#![cfg(target_os = "linux")]

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
                "{ node.dont-move = true node.dont-fallback = true node.dont-reconnect = true }",
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
    panic!("removed output did not publish recovery");
}

#[test]
#[ignore = "run scripts/check-pipewire.py to create an isolated graph"]
fn private_graph_routing_recreation_and_stop() {
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
