//! sherpa-onnx implementation of Prollyglot's streaming speech contract.

use std::{path::Path, sync::Arc};

use prollyglot_asr::{
    EngineState, InferenceDevice, ModelLocation, SpeechAudio, SpeechEngine, SpeechEngineInfo,
    SpeechError, SpeechErrorKind, SpeechEvent, SpeechHypothesis, SpeechStream, SpeechStreamConfig,
};
use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, OnlineStream};

const ENGINE_ID: &str = "sherpa-onnx-online";
const TRANSDUCER_BACKEND: &str = "sherpa-onnx-online-transducer";
const NEMOTRON_BACKEND: &str = "sherpa-onnx-online-nemotron";
const ENDPOINT_RULE_1_TRAILING_SILENCE_SECONDS: f32 = 2.4;
const ENDPOINT_RULE_2_TRAILING_SILENCE_SECONDS: f32 = 1.2;
const ENDPOINT_RULE_3_UTTERANCE_SECONDS: f32 = 20.0;
const NEMOTRON_ENDPOINT_RULE_3_UTTERANCE_SECONDS: f32 = 4.0;
const NEMOTRON_MAX_UTTERANCE_MICROS: u64 = 4_000_000;
const STREAM_PREROLL_MILLIS: u32 = 800;
const NEMOTRON_LEFT_PADDING_MILLIS: u32 = 500;
const NEMOTRON_RIGHT_PADDING_MILLIS: u32 = 800;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SherpaOnlineConfig {
    pub num_threads: i32,
}

impl Default for SherpaOnlineConfig {
    fn default() -> Self {
        Self { num_threads: 2 }
    }
}

pub struct SherpaOnlineEngine {
    info: SpeechEngineInfo,
    config: SherpaOnlineConfig,
    recognizer: Option<Arc<OnlineRecognizer>>,
    loaded_model_kind: Option<LoadedModelKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LoadedModelKind {
    Transducer,
    Nemotron,
}

impl SherpaOnlineEngine {
    pub fn new(config: SherpaOnlineConfig) -> Self {
        Self {
            info: SpeechEngineInfo {
                id: ENGINE_ID.into(),
                name: "sherpa-onnx streaming transducer".into(),
                languages: vec!["en".into()],
                streaming: true,
                supported_devices: vec![InferenceDevice::Cpu],
            },
            config: SherpaOnlineConfig {
                num_threads: config.num_threads.max(1),
            },
            recognizer: None,
            loaded_model_kind: None,
        }
    }
}

impl Default for SherpaOnlineEngine {
    fn default() -> Self {
        Self::new(SherpaOnlineConfig::default())
    }
}

impl SpeechEngine for SherpaOnlineEngine {
    fn info(&self) -> &SpeechEngineInfo {
        &self.info
    }

    fn state(&self) -> EngineState {
        if self.recognizer.is_some() {
            EngineState::Loaded
        } else {
            EngineState::Unloaded
        }
    }

    fn load_model(&mut self, model: &ModelLocation) -> Result<(), SpeechError> {
        let model_kind = match model.backend.as_str() {
            TRANSDUCER_BACKEND => LoadedModelKind::Transducer,
            NEMOTRON_BACKEND => LoadedModelKind::Nemotron,
            backend => {
                return Err(SpeechError::new(
                    SpeechErrorKind::BackendUnavailable,
                    format!(
                        "speech model {} uses unsupported backend {backend}",
                        model.id
                    ),
                    false,
                ));
            }
        };
        let encoder = required_artifact(model, "encoder")?;
        let decoder = required_artifact(model, "decoder")?;
        let joiner = required_artifact(model, "joiner")?;
        let tokens = required_artifact(model, "tokens")?;

        let mut config = OnlineRecognizerConfig::default();
        config.model_config.transducer.encoder = Some(path_string(encoder)?);
        config.model_config.transducer.decoder = Some(path_string(decoder)?);
        config.model_config.transducer.joiner = Some(path_string(joiner)?);
        config.model_config.tokens = Some(path_string(tokens)?);
        config.model_config.num_threads = self.config.num_threads;
        config.model_config.provider = Some("cpu".into());
        if model_kind == LoadedModelKind::Nemotron {
            config.model_config.modeling_unit = Some("cjkchar".into());
        }
        config.decoding_method = Some("greedy_search".into());
        config.enable_endpoint = true;
        // The Rust wrapper's numeric defaults are zero, which makes endpoint
        // detection reset a live stream before it has a useful hypothesis.
        // Preserve sherpa-onnx's documented endpoint profile explicitly.
        config.rule1_min_trailing_silence = ENDPOINT_RULE_1_TRAILING_SILENCE_SECONDS;
        config.rule2_min_trailing_silence = ENDPOINT_RULE_2_TRAILING_SILENCE_SECONDS;
        // Bound continuous multilingual phrases so live and finalized
        // translation requests stay readable and do not grow for twenty
        // seconds during pause-light news or other media.
        config.rule3_min_utterance_length = match model_kind {
            LoadedModelKind::Nemotron => NEMOTRON_ENDPOINT_RULE_3_UTTERANCE_SECONDS,
            LoadedModelKind::Transducer => ENDPOINT_RULE_3_UTTERANCE_SECONDS,
        };

        let recognizer = OnlineRecognizer::create(&config).ok_or_else(|| {
            SpeechError::new(
                SpeechErrorKind::BackendUnavailable,
                format!(
                    "sherpa-onnx could not load model {}; verify the model files and available memory",
                    model.id
                ),
                true,
            )
        })?;
        self.info.languages.clone_from(&model.languages);
        self.recognizer = Some(Arc::new(recognizer));
        self.loaded_model_kind = Some(model_kind);
        Ok(())
    }

    fn unload_model(&mut self) -> Result<(), SpeechError> {
        self.recognizer = None;
        self.loaded_model_kind = None;
        self.info.languages.clear();
        Ok(())
    }

    fn start_stream(
        &self,
        config: SpeechStreamConfig,
    ) -> Result<Box<dyn SpeechStream>, SpeechError> {
        if config.sample_rate == 0 || config.sample_rate > i32::MAX as u32 {
            return Err(SpeechError::new(
                SpeechErrorKind::InvalidAudio,
                "speech stream sample rate must fit a positive 32-bit integer",
                false,
            ));
        }
        if !self.info.languages.contains(&config.language) {
            return Err(SpeechError::new(
                SpeechErrorKind::UnsupportedLanguage,
                format!("the loaded model does not support {}", config.language),
                true,
            ));
        }
        let recognizer = Arc::clone(self.recognizer.as_ref().ok_or_else(|| {
            SpeechError::new(
                SpeechErrorKind::MissingModel,
                "load a speech model before starting transcription",
                true,
            )
        })?);
        let model_kind = self.loaded_model_kind.ok_or_else(|| {
            SpeechError::new(
                SpeechErrorKind::Internal,
                "the loaded speech model has no runtime configuration",
                false,
            )
        })?;
        let stream = recognizer.create_stream();
        configure_stream_language(&stream, model_kind, &config.language);
        let speech_stream = SherpaOnlineStream {
            recognizer,
            stream,
            model_kind,
            sample_rate: config.sample_rate,
            language: config.language,
            utterance_id: 0,
            utterance_start_micros: None,
            latest_audio_micros: 0,
            last_partial: String::new(),
            finished: false,
        };
        speech_stream.prime_stream();
        Ok(Box::new(speech_stream))
    }
}

struct SherpaOnlineStream {
    // The native stream must be destroyed before its recognizer. Struct fields
    // are dropped in declaration order, so keep this ordering intentional.
    stream: OnlineStream,
    recognizer: Arc<OnlineRecognizer>,
    model_kind: LoadedModelKind,
    sample_rate: u32,
    language: String,
    utterance_id: u64,
    utterance_start_micros: Option<u64>,
    latest_audio_micros: u64,
    last_partial: String,
    finished: bool,
}

impl SpeechStream for SherpaOnlineStream {
    fn push_audio(&mut self, audio: SpeechAudio) -> Result<Vec<SpeechEvent>, SpeechError> {
        self.ensure_open()?;
        if audio.sample_rate != self.sample_rate || audio.samples.is_empty() {
            return Err(SpeechError::new(
                SpeechErrorKind::InvalidAudio,
                format!(
                    "expected non-empty {} Hz mono audio, received {} Hz with {} samples",
                    self.sample_rate,
                    audio.sample_rate,
                    audio.samples.len()
                ),
                true,
            ));
        }
        if audio.samples.iter().any(|sample| !sample.is_finite()) {
            return Err(SpeechError::new(
                SpeechErrorKind::InvalidAudio,
                "audio contains a non-finite sample",
                true,
            ));
        }

        self.utterance_start_micros
            .get_or_insert(audio.start_micros);
        self.latest_audio_micros = self.latest_audio_micros.max(audio.end_micros);
        self.stream
            .accept_waveform(self.sample_rate as i32, &audio.samples);
        self.decode_ready();

        let mut events = Vec::new();
        let text = self.current_text();
        let has_hypothesis = !text.is_empty() || !self.last_partial.is_empty();
        if !text.is_empty() && text != self.last_partial {
            self.last_partial.clone_from(&text);
            events.push(SpeechEvent::Partial(self.hypothesis(text)));
        }
        if self.recognizer.is_endpoint(&self.stream) {
            if let Some(event) = self.final_event() {
                events.push(event);
            }
            self.recognizer.reset(&self.stream);
            configure_stream_language(&self.stream, self.model_kind, &self.language);
            self.advance_utterance();
            self.prime_stream();
        } else if nemotron_boundary_reached(
            self.model_kind,
            self.utterance_start_micros,
            self.latest_audio_micros,
            has_hypothesis,
        ) {
            // Some Nemotron streams do not surface sherpa-onnx's rule-three
            // endpoint promptly during continuous media speech. Enforce the
            // same four-second ceiling in the adapter so partial text keeps
            // reaching translation in bounded, readable phrases.
            self.flush_padding();
            self.stream.input_finished();
            self.decode_ready();
            if let Some(event) = self.final_event() {
                events.push(event);
            }
            self.start_next_stream();
        }
        Ok(events)
    }

    fn discard_utterance(&mut self, at_micros: u64) -> Result<(), SpeechError> {
        self.ensure_open()?;
        self.latest_audio_micros = self.latest_audio_micros.max(at_micros);
        self.start_next_stream();
        Ok(())
    }

    fn end_utterance(&mut self, at_micros: u64) -> Result<Vec<SpeechEvent>, SpeechError> {
        self.ensure_open()?;
        self.latest_audio_micros = self.latest_audio_micros.max(at_micros);
        self.flush_padding();
        self.stream.input_finished();
        self.decode_ready();
        let event = self.final_event().into_iter().collect();
        self.start_next_stream();
        Ok(event)
    }

    fn finish(&mut self, at_micros: u64) -> Result<Vec<SpeechEvent>, SpeechError> {
        self.ensure_open()?;
        let events = self.end_utterance(at_micros)?;
        self.finished = true;
        Ok(events)
    }
}

fn nemotron_boundary_reached(
    model_kind: LoadedModelKind,
    utterance_start_micros: Option<u64>,
    latest_audio_micros: u64,
    has_hypothesis: bool,
) -> bool {
    model_kind == LoadedModelKind::Nemotron
        && has_hypothesis
        && utterance_start_micros.is_some_and(|start| {
            latest_audio_micros.saturating_sub(start) >= NEMOTRON_MAX_UTTERANCE_MICROS
        })
}

impl SherpaOnlineStream {
    fn ensure_open(&self) -> Result<(), SpeechError> {
        if self.finished {
            Err(SpeechError::new(
                SpeechErrorKind::Internal,
                "speech stream is already finished",
                false,
            ))
        } else {
            Ok(())
        }
    }

    fn decode_ready(&self) {
        while self.recognizer.is_ready(&self.stream) {
            self.recognizer.decode(&self.stream);
        }
    }

    fn prime_stream(&self) {
        let padding_millis = match self.model_kind {
            LoadedModelKind::Transducer => STREAM_PREROLL_MILLIS,
            LoadedModelKind::Nemotron => NEMOTRON_LEFT_PADDING_MILLIS,
        };
        let sample_count = self.sample_rate as usize * padding_millis as usize / 1_000;
        self.stream
            .accept_waveform(self.sample_rate as i32, &vec![0.0; sample_count]);
        self.decode_ready();
    }

    fn flush_padding(&self) {
        if self.model_kind != LoadedModelKind::Nemotron {
            return;
        }
        let sample_count =
            self.sample_rate as usize * NEMOTRON_RIGHT_PADDING_MILLIS as usize / 1_000;
        self.stream
            .accept_waveform(self.sample_rate as i32, &vec![0.0; sample_count]);
    }

    fn current_text(&self) -> String {
        self.recognizer
            .get_result(&self.stream)
            .map(|result| result.text.trim().to_owned())
            .unwrap_or_default()
    }

    fn final_event(&self) -> Option<SpeechEvent> {
        let current = self.current_text();
        let text = if current.is_empty() {
            self.last_partial.clone()
        } else {
            current
        };
        (!text.is_empty()).then(|| SpeechEvent::Final(self.hypothesis(text)))
    }

    fn hypothesis(&self, text: String) -> SpeechHypothesis {
        SpeechHypothesis {
            utterance_id: self.utterance_id,
            text,
            start_micros: self
                .utterance_start_micros
                .unwrap_or(self.latest_audio_micros),
            end_micros: self.latest_audio_micros,
            language: self.language.clone(),
        }
    }

    fn advance_utterance(&mut self) {
        self.utterance_id = self.utterance_id.wrapping_add(1);
        self.utterance_start_micros = None;
        self.last_partial.clear();
    }

    fn start_next_stream(&mut self) {
        self.stream = self.recognizer.create_stream();
        configure_stream_language(&self.stream, self.model_kind, &self.language);
        self.advance_utterance();
        self.prime_stream();
    }
}

fn configure_stream_language(stream: &OnlineStream, model_kind: LoadedModelKind, language: &str) {
    if model_kind == LoadedModelKind::Nemotron {
        stream.set_option("language", language);
    }
}

fn required_artifact<'a>(model: &'a ModelLocation, role: &str) -> Result<&'a Path, SpeechError> {
    let path = model.artifacts.get(role).ok_or_else(|| {
        SpeechError::new(
            SpeechErrorKind::CorruptModel,
            format!("model {} is missing its {role} artifact", model.id),
            true,
        )
    })?;
    if !path.is_file() {
        return Err(SpeechError::new(
            SpeechErrorKind::CorruptModel,
            format!("model artifact {} is unavailable", path.display()),
            true,
        ));
    }
    Ok(path)
}

fn path_string(path: &Path) -> Result<String, SpeechError> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        SpeechError::new(
            SpeechErrorKind::CorruptModel,
            format!("model path {} is not valid Unicode", path.display()),
            true,
        )
    })
}

#[cfg(test)]
mod tests {
    use prollyglot_asr::{EngineState, ModelLocation, SpeechEngine, SpeechErrorKind};
    use prollyglot_model_manager::{ModelManager, initial_english_manifest};
    use sherpa_onnx::Wave;

    use super::*;

    #[test]
    fn enforces_a_bounded_nemotron_utterance_only_after_text_appears() {
        assert!(!nemotron_boundary_reached(
            LoadedModelKind::Nemotron,
            Some(1_000_000),
            4_999_999,
            true,
        ));
        assert!(nemotron_boundary_reached(
            LoadedModelKind::Nemotron,
            Some(1_000_000),
            5_000_000,
            true,
        ));
        assert!(!nemotron_boundary_reached(
            LoadedModelKind::Nemotron,
            Some(1_000_000),
            8_000_000,
            false,
        ));
        assert!(!nemotron_boundary_reached(
            LoadedModelKind::Transducer,
            Some(1_000_000),
            8_000_000,
            true,
        ));
    }

    #[test]
    fn requires_a_loaded_model_before_streaming() {
        let engine = SherpaOnlineEngine::default();
        let error = match engine.start_stream(SpeechStreamConfig::default()) {
            Ok(_) => panic!("unloaded engine should fail"),
            Err(error) => error,
        };

        assert_eq!(engine.state(), EngineState::Unloaded);
        assert_eq!(error.kind, SpeechErrorKind::MissingModel);
    }

    #[test]
    fn rejects_incomplete_model_locations() {
        let directory = tempfile::tempdir().expect("temporary model root");
        let mut engine = SherpaOnlineEngine::default();
        let error = engine
            .load_model(&ModelLocation {
                id: "incomplete".into(),
                backend: TRANSDUCER_BACKEND.into(),
                languages: vec!["en".into()],
                directory: directory.path().into(),
                artifacts: Default::default(),
            })
            .expect_err("missing artifacts");

        assert_eq!(error.kind, SpeechErrorKind::CorruptModel);
    }

    #[test]
    #[ignore = "downloads 45 MB and loads the native sherpa-onnx runtime"]
    fn loads_the_pinned_english_model_and_streams_audio() {
        let directory = tempfile::tempdir().expect("temporary model root");
        let manager = ModelManager::new(directory.path());
        let manifest = initial_english_manifest().expect("built-in manifest");
        let location = manager
            .install(&manifest, |_| {})
            .expect("download pinned model");
        let mut engine = SherpaOnlineEngine::default();

        engine.load_model(&location).expect("load model");
        let mut stream = engine
            .start_stream(SpeechStreamConfig::default())
            .expect("start stream");
        let events = stream
            .push_audio(SpeechAudio {
                start_micros: 0,
                end_micros: 100_000,
                sample_rate: 16_000,
                samples: vec![0.0; 1_600],
            })
            .expect("accept silence");
        let final_events = stream.finish(100_000).expect("finish stream");

        assert_eq!(engine.state(), EngineState::Loaded);
        assert!(events.is_empty());
        assert!(final_events.is_empty());
    }

    #[test]
    #[ignore = "downloads 45 MB and transcribes the official model reference WAV"]
    fn transcribes_the_pinned_models_reference_speech() {
        let wave_path = std::env::var("PROLLYGLOT_TEST_WAV")
            .expect("set PROLLYGLOT_TEST_WAV to the pinned model repository's test_wavs/0.wav");
        let wave = Wave::read(&wave_path).expect("read mono reference WAV");
        assert_eq!(wave.sample_rate(), 16_000, "reference WAV sample rate");

        let directory = tempfile::tempdir().expect("temporary model root");
        let manager = ModelManager::new(directory.path());
        let manifest = initial_english_manifest().expect("built-in manifest");
        let location = manager
            .install(&manifest, |_| {})
            .expect("download pinned model");
        let mut engine = SherpaOnlineEngine::default();
        engine.load_model(&location).expect("load model");
        let mut stream = engine
            .start_stream(SpeechStreamConfig::default())
            .expect("start stream");

        let mut partial_count = 0_usize;
        let mut finals = Vec::new();
        let wave_duration = wave.num_samples() as u64 * 1_000_000 / wave.sample_rate() as u64;
        for pass in 0..2_u64 {
            let pass_start = pass * (wave_duration + 500_000);
            for (chunk_index, samples) in wave.samples().chunks(1_600).enumerate() {
                let start_micros = pass_start + chunk_index as u64 * 100_000;
                let end_micros =
                    start_micros + samples.len() as u64 * 1_000_000 / wave.sample_rate() as u64;
                for event in stream
                    .push_audio(SpeechAudio {
                        start_micros,
                        end_micros,
                        sample_rate: wave.sample_rate() as u32,
                        samples: samples.to_vec(),
                    })
                    .expect("stream reference audio")
                {
                    match event {
                        SpeechEvent::Partial(_) => partial_count += 1,
                        SpeechEvent::Final(result) => finals.push(result.text),
                    }
                }
            }
            let pass_end = pass_start + wave_duration;
            let ending_events = if pass == 0 {
                stream
                    .end_utterance(pass_end)
                    .expect("finalize first reference utterance")
            } else {
                stream
                    .finish(pass_end)
                    .expect("finish second reference utterance")
            };
            for event in ending_events {
                if let SpeechEvent::Final(result) = event {
                    finals.push(result.text);
                }
            }
        }

        let recognized = finals.join(" ").to_lowercase();
        assert!(partial_count > 0, "stream should produce incremental text");
        assert!(
            recognized.matches("after early nightfall").count() == 2
                && recognized.contains("yellow lamps")
                && recognized.contains("squalid quarter"),
            "unexpected reference transcription: {recognized}"
        );
    }
}
