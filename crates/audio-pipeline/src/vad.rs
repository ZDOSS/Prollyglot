#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceActivityConfig {
    pub rms_threshold: f32,
    pub attack_chunks: u16,
    pub release_chunks: u16,
}

impl Default for VoiceActivityConfig {
    fn default() -> Self {
        Self {
            // Desktop PCM is already clean and can contain quiet dialogue.
            // Err toward recall here; the recognizer still decides whether
            // the admitted signal contains speech.
            rms_threshold: 0.001,
            attack_chunks: 1,
            // With the default 100 ms pipeline chunk this keeps 600 ms of
            // trailing context instead of cutting at the first short pause.
            release_chunks: 6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceActivity {
    Silence,
    Started,
    Speech,
    Ended,
}

/// A deliberately small, backend-independent energy gate. It provides useful
/// silence suppression and phrase boundaries now and can later be replaced by
/// a neural VAD without changing ASR or transcript contracts.
pub struct EnergyVoiceDetector {
    config: VoiceActivityConfig,
    active: bool,
    consecutive_signal: u16,
    consecutive_silence: u16,
}

impl EnergyVoiceDetector {
    pub fn new(mut config: VoiceActivityConfig) -> Self {
        config.rms_threshold = config.rms_threshold.max(0.0);
        config.attack_chunks = config.attack_chunks.max(1);
        config.release_chunks = config.release_chunks.max(1);
        Self {
            config,
            active: false,
            consecutive_signal: 0,
            consecutive_silence: 0,
        }
    }

    pub fn observe(&mut self, samples: &[f32]) -> (VoiceActivity, f32) {
        let rms = root_mean_square(samples);
        if rms >= self.config.rms_threshold {
            self.consecutive_silence = 0;
            if self.active {
                return (VoiceActivity::Speech, rms);
            }
            self.consecutive_signal = self.consecutive_signal.saturating_add(1);
            if self.consecutive_signal >= self.config.attack_chunks {
                self.active = true;
                self.consecutive_signal = 0;
                return (VoiceActivity::Started, rms);
            }
            return (VoiceActivity::Silence, rms);
        }

        self.consecutive_signal = 0;
        if !self.active {
            return (VoiceActivity::Silence, rms);
        }
        self.consecutive_silence = self.consecutive_silence.saturating_add(1);
        if self.consecutive_silence >= self.config.release_chunks {
            self.active = false;
            self.consecutive_silence = 0;
            (VoiceActivity::Ended, rms)
        } else {
            (VoiceActivity::Speech, rms)
        }
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.consecutive_signal = 0;
        self.consecutive_silence = 0;
    }

    pub const fn is_active(&self) -> bool {
        self.active
    }
}

fn root_mean_square(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean_square =
        samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32;
    mean_square.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_and_release_debounce_phrase_boundaries() {
        let mut detector = EnergyVoiceDetector::new(VoiceActivityConfig {
            rms_threshold: 0.1,
            attack_chunks: 2,
            release_chunks: 2,
        });

        assert_eq!(detector.observe(&[0.2; 8]).0, VoiceActivity::Silence);
        assert_eq!(detector.observe(&[0.2; 8]).0, VoiceActivity::Started);
        assert_eq!(detector.observe(&[0.0; 8]).0, VoiceActivity::Speech);
        assert_eq!(detector.observe(&[0.0; 8]).0, VoiceActivity::Ended);
        assert_eq!(detector.observe(&[0.0; 8]).0, VoiceActivity::Silence);
    }

    #[test]
    fn defaults_admit_quiet_digital_speech_and_keep_trailing_context() {
        let mut detector = EnergyVoiceDetector::new(VoiceActivityConfig::default());

        assert_eq!(detector.observe(&[0.0005; 8]).0, VoiceActivity::Silence);
        assert_eq!(detector.observe(&[0.0015; 8]).0, VoiceActivity::Started);
        for _ in 0..5 {
            assert_eq!(detector.observe(&[0.0; 8]).0, VoiceActivity::Speech);
        }
        assert_eq!(detector.observe(&[0.0; 8]).0, VoiceActivity::Ended);
    }
}
