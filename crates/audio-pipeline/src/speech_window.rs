use std::collections::VecDeque;

use crate::{ProcessedAudioChunk, VoiceActivity};

/// Routes VAD-tagged chunks into an utterance while retaining a bounded amount
/// of quiet audio before speech begins and the release chunk after it ends.
pub struct SpeechChunkRouter {
    pre_roll_chunks: usize,
    pre_roll: VecDeque<ProcessedAudioChunk>,
}

pub struct RoutedSpeechChunks {
    pub chunks: Vec<ProcessedAudioChunk>,
    pub end_utterance: bool,
}

impl SpeechChunkRouter {
    pub fn new(pre_roll_chunks: usize) -> Self {
        Self {
            pre_roll_chunks,
            pre_roll: VecDeque::with_capacity(pre_roll_chunks),
        }
    }

    pub fn route(&mut self, chunk: ProcessedAudioChunk) -> RoutedSpeechChunks {
        match chunk.voice_activity {
            VoiceActivity::Silence => {
                if self.pre_roll_chunks > 0 {
                    self.pre_roll.push_back(chunk);
                    while self.pre_roll.len() > self.pre_roll_chunks {
                        self.pre_roll.pop_front();
                    }
                }
                RoutedSpeechChunks {
                    chunks: Vec::new(),
                    end_utterance: false,
                }
            }
            VoiceActivity::Started => {
                let mut chunks = self.pre_roll.drain(..).collect::<Vec<_>>();
                chunks.push(chunk);
                RoutedSpeechChunks {
                    chunks,
                    end_utterance: false,
                }
            }
            VoiceActivity::Speech => RoutedSpeechChunks {
                chunks: vec![chunk],
                end_utterance: false,
            },
            VoiceActivity::Ended => RoutedSpeechChunks {
                // Feed the release chunk before finalizing so the decoder gets
                // the full trailing context used to detect the phrase ending.
                chunks: vec![chunk],
                end_utterance: true,
            },
        }
    }

    pub fn reset(&mut self) {
        self.pre_roll.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(index: u64, voice_activity: VoiceActivity) -> ProcessedAudioChunk {
        ProcessedAudioChunk {
            start_micros: index * 100_000,
            end_micros: (index + 1) * 100_000,
            sample_rate: 16_000,
            samples: vec![index as f32; 1_600],
            voice_activity,
            rms: 0.0,
        }
    }

    #[test]
    fn keeps_three_leading_chunks_and_the_release_chunk() {
        let mut router = SpeechChunkRouter::new(3);
        for index in 0..5 {
            assert!(
                router
                    .route(chunk(index, VoiceActivity::Silence))
                    .chunks
                    .is_empty()
            );
        }

        let started = router.route(chunk(5, VoiceActivity::Started));
        assert_eq!(
            started
                .chunks
                .iter()
                .map(|chunk| chunk.start_micros)
                .collect::<Vec<_>>(),
            vec![200_000, 300_000, 400_000, 500_000]
        );
        assert!(!started.end_utterance);

        let ended = router.route(chunk(6, VoiceActivity::Ended));
        assert_eq!(ended.chunks.len(), 1);
        assert_eq!(ended.chunks[0].start_micros, 600_000);
        assert!(ended.end_utterance);
    }

    #[test]
    fn reset_discards_pre_roll_from_the_previous_stream() {
        let mut router = SpeechChunkRouter::new(3);
        router.route(chunk(0, VoiceActivity::Silence));
        router.reset();

        let started = router.route(chunk(1, VoiceActivity::Started));
        assert_eq!(started.chunks.len(), 1);
        assert_eq!(started.chunks[0].start_micros, 100_000);
    }
}
