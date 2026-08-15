use std::{sync::Arc, time::Duration};

use crossbeam_channel::{Receiver, RecvTimeoutError};
use parking_lot::Mutex;
use prollyglot_asr::{SpeechAudio, SpeechEngine, SpeechEvent, SpeechStream, SpeechStreamConfig};
use prollyglot_asr_sherpa::SherpaOnlineEngine;
use prollyglot_audio_pipeline::{AudioPipeline, AudioPipelineConfig, SpeechChunkRouter};
use prollyglot_core::AudioFrame;
use prollyglot_model_manager::{ModelManager, speech_manifest};
use prollyglot_transcript::{TranscriptMutation, TranscriptStore};
use tauri::{AppHandle, Emitter};

const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(100);
const SPEECH_PREROLL_CHUNKS: usize = 5;

pub fn prepare_stream(
    models_root: std::path::PathBuf,
    model_id: &str,
    language: String,
) -> Result<Box<dyn SpeechStream>, String> {
    let manifest = speech_manifest(model_id).map_err(|error| error.to_string())?;
    let manager = ModelManager::new(models_root);
    let location = manager
        .location(&manifest)
        .map_err(|error| error.to_string())?;
    let mut engine = SherpaOnlineEngine::default();
    engine
        .load_model(&location)
        .map_err(|error| error.to_string())?;
    engine
        .start_stream(SpeechStreamConfig {
            language,
            ..SpeechStreamConfig::default()
        })
        .map_err(|error| error.to_string())
}

pub fn run(
    app: AppHandle,
    audio: Receiver<AudioFrame>,
    mut stream: Box<dyn SpeechStream>,
    transcript: Arc<Mutex<TranscriptStore>>,
) -> Result<(), String> {
    run_inner(&app, &audio, stream.as_mut(), &transcript).inspect_err(|error| {
        tracing::error!(%error, "transcription worker failed");
    })
}

fn run_inner(
    app: &AppHandle,
    audio: &Receiver<AudioFrame>,
    stream: &mut dyn SpeechStream,
    transcript: &Arc<Mutex<TranscriptStore>>,
) -> Result<(), String> {
    let mut pipeline =
        AudioPipeline::new(AudioPipelineConfig::default()).map_err(|error| error.to_string())?;
    let mut latest_audio_micros = 0_u64;
    let mut speech_router = SpeechChunkRouter::new(SPEECH_PREROLL_CHUNKS);

    loop {
        match audio.recv_timeout(WORKER_POLL_INTERVAL) {
            Ok(frame) => {
                let frame_start = frame
                    .captured_at_micros
                    .saturating_sub(frame.duration_micros());
                let report = pipeline
                    .push_frame(frame)
                    .map_err(|error| error.to_string())?;
                if report.stream_reset {
                    speech_router.reset();
                    // A discontinuity means part of this utterance is gone.
                    // Finalizing it is both misleading and particularly
                    // expensive for Nemotron; abandon only the provisional
                    // hypothesis and resume with current audio instead.
                    stream
                        .discard_utterance(frame_start)
                        .map_err(|error| error.to_string())?;
                    discard_provisional(app, transcript);
                }
                if report.dropped_samples > 0 {
                    tracing::warn!(
                        dropped_samples = report.dropped_samples,
                        total_dropped_samples = report.total_dropped_samples,
                        "transcription audio buffer dropped old samples"
                    );
                }

                while let Some(chunk) = pipeline.next_chunk() {
                    latest_audio_micros = latest_audio_micros.max(chunk.end_micros);
                    let end_micros = chunk.end_micros;
                    let routed = speech_router.route(chunk);
                    for chunk in routed.chunks {
                        let events = stream
                            .push_audio(SpeechAudio {
                                start_micros: chunk.start_micros,
                                end_micros: chunk.end_micros,
                                sample_rate: chunk.sample_rate,
                                samples: chunk.samples,
                            })
                            .map_err(|error| error.to_string())?;
                        update_transcript(app, transcript, events);
                    }
                    if routed.end_utterance {
                        let events = stream
                            .end_utterance(end_micros)
                            .map_err(|error| error.to_string())?;
                        update_transcript(app, transcript, events);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    let events = stream
        .finish(latest_audio_micros)
        .map_err(|error| error.to_string())?;
    update_transcript(app, transcript, events);
    Ok(())
}

fn discard_provisional(app: &AppHandle, transcript: &Arc<Mutex<TranscriptStore>>) {
    let mut store = transcript.lock();
    let mutation = store.discard_provisional();
    let snapshot = (mutation != TranscriptMutation::Unchanged).then(|| store.snapshot().clone());
    drop(store);

    if let Some(snapshot) = snapshot
        && let Err(error) = app.emit("transcript-update", snapshot)
    {
        tracing::warn!(%error, "could not emit discarded provisional transcript");
    }
}

fn update_transcript(
    app: &AppHandle,
    transcript: &Arc<Mutex<TranscriptStore>>,
    events: Vec<SpeechEvent>,
) {
    for event in events {
        let mut store = transcript.lock();
        if store.apply(event) == TranscriptMutation::Unchanged {
            continue;
        }
        let snapshot = store.snapshot().clone();
        drop(store);

        if let Err(error) = app.emit("transcript-update", snapshot) {
            tracing::warn!(%error, "could not emit transcript update");
        }
    }
}
