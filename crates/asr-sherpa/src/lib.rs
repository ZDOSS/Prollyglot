//! sherpa-onnx implementation of Prollyglot's streaming speech contract.

use std::{path::Path, sync::Arc};

use prollyglot_asr::{
    EngineState, InferenceDevice, ModelLocation, SpeechAudio, SpeechEngine, SpeechEngineInfo,
    SpeechError, SpeechErrorKind, SpeechEvent, SpeechHypothesis, SpeechStream, SpeechStreamConfig,
};
use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, OnlineStream};

const ENGINE_ID: &str = "sherpa-onnx-online-transducer";

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
        config.decoding_method = Some("greedy_search".into());
        config.enable_endpoint = true;

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
        self.recognizer = Some(Arc::new(recognizer));
        Ok(())
    }

    fn unload_model(&mut self) -> Result<(), SpeechError> {
        self.recognizer = None;
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
                "load an English caption model before starting transcription",
                true,
            )
        })?);
        let stream = recognizer.create_stream();
        Ok(Box::new(SherpaOnlineStream {
            recognizer,
            stream,
            sample_rate: config.sample_rate,
            language: config.language,
            utterance_id: 0,
            utterance_start_micros: None,
            latest_audio_micros: 0,
            last_partial: String::new(),
            finished: false,
        }))
    }
}

struct SherpaOnlineStream {
    // The native stream must be destroyed before its recognizer. Struct fields
    // are dropped in declaration order, so keep this ordering intentional.
    stream: OnlineStream,
    recognizer: Arc<OnlineRecognizer>,
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
        if !text.is_empty() && text != self.last_partial {
            self.last_partial.clone_from(&text);
            events.push(SpeechEvent::Partial(self.hypothesis(text)));
        }
        if self.recognizer.is_endpoint(&self.stream) {
            if let Some(event) = self.final_event() {
                events.push(event);
            }
            self.recognizer.reset(&self.stream);
            self.advance_utterance();
        }
        Ok(events)
    }

    fn end_utterance(&mut self, at_micros: u64) -> Result<Vec<SpeechEvent>, SpeechError> {
        self.ensure_open()?;
        self.latest_audio_micros = self.latest_audio_micros.max(at_micros);
        self.stream.input_finished();
        self.decode_ready();
        let event = self.final_event().into_iter().collect();
        self.stream = self.recognizer.create_stream();
        self.advance_utterance();
        Ok(event)
    }

    fn finish(&mut self, at_micros: u64) -> Result<Vec<SpeechEvent>, SpeechError> {
        self.ensure_open()?;
        let events = self.end_utterance(at_micros)?;
        self.finished = true;
        Ok(events)
    }
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

    use super::*;

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
}
