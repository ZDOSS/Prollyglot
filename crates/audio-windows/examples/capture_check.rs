//! Explicit native verification with two quiet synthetic tone applications.
//! Captured PCM stays in memory; output contains only signal measurements.
#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("capture_check requires native Windows");
    std::process::exit(1);
}

#[cfg(target_os = "windows")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    native::run()
}

#[cfg(target_os = "windows")]
mod native {
    use crossbeam_channel::{Receiver, bounded};
    use prollyglot_audio_windows::WindowsAudioCaptureBackend;
    use prollyglot_core::{
        AudioCaptureBackend, CaptureEvent, CaptureRecoveryKind, CaptureSelection, SourceId,
    };
    use std::{
        error::Error,
        path::Path,
        process::{Child, Command},
        thread,
        time::{Duration, Instant},
    };
    use windows::{
        Win32::Media::Audio::{
            PlaySoundW, SND_ASYNC, SND_FLAGS, SND_LOOP, SND_MEMORY, SND_NODEFAULT,
        },
        core::PCWSTR,
    };

    const A: f64 = 997.0;
    const B: f64 = 1733.0;

    struct Player(Child);
    impl Player {
        fn start(path: &Path, frequency: f64) -> Result<Self, Box<dyn Error>> {
            Ok(Self(
                Command::new(path)
                    .args(["--tone", &frequency.to_string()])
                    .spawn()?,
            ))
        }
    }
    impl Drop for Player {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn tone(frequency: f64) -> Result<(), Box<dyn Error>> {
        if ![A, B].contains(&frequency) {
            return Err("unknown fixture frequency".into());
        }
        let rate = 48_000_u32;
        let length = rate * 2;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(length + 36).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 2).to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&length.to_le_bytes());
        for index in 0..rate {
            let sample = (0.004
                * i16::MAX as f64
                * (std::f64::consts::TAU * frequency * index as f64 / rate as f64).sin())
                as i16;
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        // SND_MEMORY consumes WAV bytes. Keep their address alive until the
        // asynchronous playback is stopped; the fixture also expires itself.
        if !unsafe {
            PlaySoundW(
                PCWSTR(wav.as_ptr().cast()),
                None,
                SND_MEMORY | SND_ASYNC | SND_LOOP | SND_NODEFAULT,
            )
        }
        .as_bool()
        {
            return Err("Windows could not play the synthetic fixture".into());
        }
        thread::sleep(Duration::from_secs(30));
        let _ = unsafe { PlaySoundW(PCWSTR::null(), None, SND_FLAGS(0)) };
        Ok(())
    }

    fn application(backend: &WindowsAudioCaptureBackend) -> Result<SourceId, Box<dyn Error>> {
        let started = Instant::now();
        loop {
            if let Some(app) = backend
                .source_snapshot()?
                .applications
                .into_iter()
                .find(|app| app.name.eq_ignore_ascii_case("prollyglot-audio-fixture-a"))
            {
                if app.instance_count != 1 {
                    return Err("fixture identity is ambiguous".into());
                }
                return Ok(app.id);
            }
            if started.elapsed() > Duration::from_secs(8) {
                return Err("fixture did not appear in the audio source list".into());
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    struct Probe {
        samples: Vec<f32>,
        rate: u32,
        last: Option<(u64, u64)>,
        discontinuity: bool,
    }
    fn collect(
        events: &Receiver<CaptureEvent>,
        previous: Option<(u64, u64)>,
    ) -> Result<Probe, Box<dyn Error>> {
        let started = Instant::now();
        let mut probe = Probe {
            samples: Vec::new(),
            rate: 0,
            last: previous,
            discontinuity: false,
        };
        while started.elapsed() < Duration::from_secs(8) {
            match events.recv_timeout(Duration::from_millis(250)) {
                Ok(CaptureEvent::Frame(frame)) => {
                    if let Some((sequence, timestamp)) = probe.last
                        && (frame.sequence <= sequence || frame.captured_at_micros < timestamp)
                    {
                        return Err("capture clock moved backwards".into());
                    }
                    probe.last = Some((frame.sequence, frame.captured_at_micros));
                    probe.discontinuity |= frame.discontinuity;
                    if probe.rate != 0 && probe.rate != frame.sample_rate {
                        return Err("unexpected fixture format change".into());
                    }
                    probe.rate = frame.sample_rate;
                    probe.samples.extend(frame.samples);
                    if probe.samples.len() >= probe.rate as usize {
                        return Ok(probe);
                    }
                }
                Ok(CaptureEvent::Error(error)) => return Err(error.into()),
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    return Err("capture channel closed".into());
                }
                _ => {}
            }
        }
        Err("capture did not supply one second of PCM".into())
    }

    fn amplitude(probe: &Probe, frequency: f64) -> f64 {
        let (mut real, mut imaginary) = (0.0, 0.0);
        for (index, sample) in probe.samples.iter().enumerate() {
            let phase = std::f64::consts::TAU * frequency * index as f64 / probe.rate as f64;
            real += f64::from(*sample) * phase.cos();
            imaginary += f64::from(*sample) * phase.sin();
        }
        2.0 * real.hypot(imaginary) / probe.samples.len() as f64
    }

    fn check(probe: &Probe, both: bool, mode: &str) -> Result<(), Box<dyn Error>> {
        let a = amplitude(probe, A);
        let b = amplitude(probe, B);
        let passed = a > 0.00001 && if both { b > 0.00001 } else { b < a * 0.1 };
        println!(
            "{}",
            serde_json::json!({"mode":mode,"passed":passed,"selectedTone":a,"otherTone":b,
            "sampleRate":probe.rate,"discontinuity":probe.discontinuity})
        );
        if !passed {
            return Err(format!("{mode}: source isolation/signal check failed").into());
        }
        Ok(())
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        let args: Vec<_> = std::env::args().skip(1).collect();
        if args.first().is_some_and(|a| a == "--tone") {
            return tone(args.get(1).ok_or("missing tone")?.parse()?);
        }
        if args != ["--run"] {
            return Err(
                "usage: capture_check --run (plays quiet synthetic tones for about 10 seconds)"
                    .into(),
            );
        }
        let fixtures = tempfile::tempdir()?;
        let player_a = fixtures.path().join("prollyglot-audio-fixture-a.exe");
        let player_b = fixtures.path().join("prollyglot-audio-fixture-b.exe");
        let executable = std::env::current_exe()?;
        std::fs::copy(&executable, &player_a)?;
        std::fs::copy(&executable, &player_b)?;
        let mut a = Some(Player::start(&player_a, A)?);
        let _b = Player::start(&player_b, B)?;
        let backend = WindowsAudioCaptureBackend::new();
        let source_id = application(&backend)?;
        let device_id = backend
            .source_snapshot()?
            .playback_devices
            .into_iter()
            .find(|d| d.is_default)
            .ok_or("no default playback device")?
            .id;
        for (mode, selection) in [
            ("default", CaptureSelection::SystemDefault),
            ("device", CaptureSelection::SystemOutput { device_id }),
            (
                "application",
                CaptureSelection::Application {
                    source_id: source_id.clone(),
                },
            ),
        ] {
            let (sender, receiver) = bounded(512);
            let mut capture = backend.start_capture(selection, sender)?;
            let first = collect(&receiver, None)?;
            check(&first, mode != "application", mode)?;
            if mode == "application" {
                drop(a.take());
                let waiting = Instant::now();
                loop {
                    match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(CaptureEvent::Recovery(recovery))
                            if matches!(
                                recovery.kind,
                                CaptureRecoveryKind::ApplicationExited
                                    | CaptureRecoveryKind::ApplicationUnavailable
                            ) =>
                        {
                            break;
                        }
                        Ok(CaptureEvent::Error(error)) => return Err(error.into()),
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                            return Err("capture channel closed during recovery".into());
                        }
                        _ => {}
                    }
                    if waiting.elapsed() > Duration::from_secs(8) {
                        return Err("closed application did not enter Waiting".into());
                    }
                }
                a = Some(Player::start(&player_a, A)?);
                let recovered = collect(&receiver, first.last)?;
                check(&recovered, false, "application-restarted")?;
                if !recovered.discontinuity {
                    return Err("recovery omitted discontinuity".into());
                }
            }
            let stop = Instant::now();
            capture.stop()?;
            let stop_ms = stop.elapsed().as_secs_f64() * 1000.0;
            println!("{}", serde_json::json!({"mode":mode,"stopMs":stop_ms}));
            if stop_ms > 2_000.0 {
                return Err("capture Stop exceeded two seconds".into());
            }
        }
        drop(a);
        Ok(())
    }
}
