//! In-memory transcript state with replaceable partials and immutable finals.

use std::collections::HashSet;

use prollyglot_asr::{SpeechEvent, SpeechHypothesis};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub utterance_id: u64,
    pub start_micros: u64,
    pub end_micros: u64,
    pub source_language: String,
    pub text: String,
    pub is_final: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSnapshot {
    pub revision: u64,
    pub provisional: Option<TranscriptSegment>,
    pub committed: Vec<TranscriptSegment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptMutation {
    Unchanged,
    ProvisionalChanged,
    SegmentCommitted,
    Cleared,
}

#[derive(Default)]
pub struct TranscriptStore {
    snapshot: TranscriptSnapshot,
    finalized_utterances: HashSet<u64>,
}

/// Returns a bounded run of recent utterances for the live caption overlay.
///
/// Finalized utterances remain separate so the UI can give pause-based turns
/// their own visual lines. A long gap starts a new run rather than resurrecting
/// stale captions when speech resumes.
pub fn recent_caption_lines(
    snapshot: &TranscriptSnapshot,
    max_segments: usize,
    max_gap_micros: u64,
) -> Vec<String> {
    if max_segments == 0 {
        return Vec::new();
    }

    let mut recent = Vec::<&TranscriptSegment>::with_capacity(max_segments);
    let mut next_start = None;

    if let Some(provisional) = snapshot
        .provisional
        .as_ref()
        .filter(|segment| !segment.text.is_empty())
    {
        recent.push(provisional);
        next_start = Some(provisional.start_micros);
    }

    for committed in snapshot.committed.iter().rev() {
        if recent.len() >= max_segments {
            break;
        }
        if let Some(start) = next_start
            && start.saturating_sub(committed.end_micros) > max_gap_micros
        {
            break;
        }
        recent.push(committed);
        next_start = Some(committed.start_micros);
    }

    recent.reverse();
    recent
        .into_iter()
        .map(|segment| segment.text.clone())
        .collect()
}

impl TranscriptStore {
    pub fn snapshot(&self) -> &TranscriptSnapshot {
        &self.snapshot
    }

    pub fn apply(&mut self, event: SpeechEvent) -> TranscriptMutation {
        match event {
            SpeechEvent::Partial(hypothesis) => self.apply_partial(hypothesis),
            SpeechEvent::Final(hypothesis) => self.apply_final(hypothesis),
        }
    }

    pub fn clear(&mut self) -> TranscriptMutation {
        if self.snapshot.provisional.is_none()
            && self.snapshot.committed.is_empty()
            && self.finalized_utterances.is_empty()
        {
            return TranscriptMutation::Unchanged;
        }
        self.snapshot.provisional = None;
        self.snapshot.committed.clear();
        self.finalized_utterances.clear();
        self.bump_revision();
        TranscriptMutation::Cleared
    }

    /// Removes only the replaceable hypothesis when its source audio became
    /// discontinuous. Already committed transcript segments remain intact.
    pub fn discard_provisional(&mut self) -> TranscriptMutation {
        if self.snapshot.provisional.take().is_none() {
            return TranscriptMutation::Unchanged;
        }
        self.bump_revision();
        TranscriptMutation::ProvisionalChanged
    }

    fn apply_partial(&mut self, hypothesis: SpeechHypothesis) -> TranscriptMutation {
        if self.finalized_utterances.contains(&hypothesis.utterance_id) {
            return TranscriptMutation::Unchanged;
        }
        let next = segment(hypothesis, false);
        let next = (!next.text.is_empty()).then_some(next);
        if self.snapshot.provisional == next {
            return TranscriptMutation::Unchanged;
        }
        self.snapshot.provisional = next;
        self.bump_revision();
        TranscriptMutation::ProvisionalChanged
    }

    fn apply_final(&mut self, hypothesis: SpeechHypothesis) -> TranscriptMutation {
        if !self.finalized_utterances.insert(hypothesis.utterance_id) {
            return TranscriptMutation::Unchanged;
        }

        if self
            .snapshot
            .provisional
            .as_ref()
            .is_some_and(|partial| partial.utterance_id == hypothesis.utterance_id)
        {
            self.snapshot.provisional = None;
        }

        let final_segment = segment(hypothesis, true);
        if final_segment.text.is_empty() {
            self.bump_revision();
            return TranscriptMutation::ProvisionalChanged;
        }
        self.snapshot.committed.push(final_segment);
        self.bump_revision();
        TranscriptMutation::SegmentCommitted
    }

    fn bump_revision(&mut self) {
        self.snapshot.revision = self.snapshot.revision.wrapping_add(1);
    }
}

fn segment(hypothesis: SpeechHypothesis, is_final: bool) -> TranscriptSegment {
    TranscriptSegment {
        utterance_id: hypothesis.utterance_id,
        start_micros: hypothesis.start_micros,
        end_micros: hypothesis.end_micros.max(hypothesis.start_micros),
        source_language: hypothesis.language,
        text: hypothesis.text.trim().to_owned(),
        is_final,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hypothesis(utterance_id: u64, text: &str, end_micros: u64) -> SpeechHypothesis {
        SpeechHypothesis {
            utterance_id,
            text: text.into(),
            start_micros: 100,
            end_micros,
            language: "en".into(),
        }
    }

    #[test]
    fn partials_replace_only_the_provisional_segment() {
        let mut store = TranscriptStore::default();
        store.apply(SpeechEvent::Partial(hypothesis(1, "good", 200)));
        store.apply(SpeechEvent::Partial(hypothesis(1, "good morning", 300)));

        assert_eq!(store.snapshot().committed, Vec::new());
        assert_eq!(
            store
                .snapshot()
                .provisional
                .as_ref()
                .map(|item| item.text.as_str()),
            Some("good morning")
        );
        assert_eq!(store.snapshot().revision, 2);
    }

    #[test]
    fn final_text_is_committed_once_and_never_churns() {
        let mut store = TranscriptStore::default();
        store.apply(SpeechEvent::Partial(hypothesis(7, "we are", 200)));
        assert_eq!(
            store.apply(SpeechEvent::Final(hypothesis(7, "we are live", 300))),
            TranscriptMutation::SegmentCommitted
        );
        assert_eq!(
            store.apply(SpeechEvent::Final(hypothesis(7, "changed", 400))),
            TranscriptMutation::Unchanged
        );
        assert_eq!(
            store.apply(SpeechEvent::Partial(hypothesis(7, "changed again", 500))),
            TranscriptMutation::Unchanged
        );

        assert!(store.snapshot().provisional.is_none());
        assert_eq!(store.snapshot().committed.len(), 1);
        assert_eq!(store.snapshot().committed[0].text, "we are live");
    }

    #[test]
    fn timestamps_are_never_negative_in_duration() {
        let mut store = TranscriptStore::default();
        let mut result = hypothesis(1, "hello", 50);
        result.start_micros = 100;
        store.apply(SpeechEvent::Final(result));

        assert_eq!(store.snapshot().committed[0].end_micros, 100);
    }

    #[test]
    fn empty_final_clears_matching_partial_without_committing() {
        let mut store = TranscriptStore::default();
        store.apply(SpeechEvent::Partial(hypothesis(1, "noise", 200)));
        store.apply(SpeechEvent::Final(hypothesis(1, "  ", 300)));

        assert!(store.snapshot().provisional.is_none());
        assert!(store.snapshot().committed.is_empty());
    }

    #[test]
    fn discarding_a_provisional_segment_preserves_committed_history() {
        let mut store = TranscriptStore::default();
        store.apply(SpeechEvent::Final(hypothesis(1, "finished", 200)));
        store.apply(SpeechEvent::Partial(hypothesis(2, "unfinished", 300)));

        assert_eq!(
            store.discard_provisional(),
            TranscriptMutation::ProvisionalChanged
        );
        assert!(store.snapshot().provisional.is_none());
        assert_eq!(store.snapshot().committed.len(), 1);
        assert_eq!(store.snapshot().committed[0].text, "finished");
    }

    #[test]
    fn clearing_an_empty_final_resets_utterance_identity_for_the_next_session() {
        let mut store = TranscriptStore::default();
        store.apply(SpeechEvent::Final(hypothesis(0, "", 200)));

        assert_eq!(store.clear(), TranscriptMutation::Cleared);
        assert_eq!(
            store.apply(SpeechEvent::Final(hypothesis(0, "new session", 300))),
            TranscriptMutation::SegmentCommitted
        );
        assert_eq!(store.snapshot().committed[0].text, "new session");
    }

    #[test]
    fn recent_caption_lines_keep_pause_bounded_turns_separate() {
        let snapshot = TranscriptSnapshot {
            revision: 4,
            committed: vec![
                TranscriptSegment {
                    utterance_id: 1,
                    start_micros: 0,
                    end_micros: 1_000_000,
                    source_language: "en".into(),
                    text: "Are you going?".into(),
                    is_final: true,
                },
                TranscriptSegment {
                    utterance_id: 2,
                    start_micros: 1_300_000,
                    end_micros: 2_000_000,
                    source_language: "en".into(),
                    text: "Yeah, probably.".into(),
                    is_final: true,
                },
            ],
            provisional: Some(TranscriptSegment {
                utterance_id: 3,
                start_micros: 2_200_000,
                end_micros: 2_600_000,
                source_language: "en".into(),
                text: "Okay".into(),
                is_final: false,
            }),
        };

        assert_eq!(
            recent_caption_lines(&snapshot, 3, 2_000_000),
            vec!["Are you going?", "Yeah, probably.", "Okay"]
        );
        assert_eq!(
            recent_caption_lines(&snapshot, 2, 2_000_000),
            vec!["Yeah, probably.", "Okay"]
        );
    }

    #[test]
    fn recent_caption_lines_do_not_restore_stale_context() {
        let snapshot = TranscriptSnapshot {
            revision: 2,
            committed: vec![TranscriptSegment {
                utterance_id: 1,
                start_micros: 0,
                end_micros: 1_000_000,
                source_language: "en".into(),
                text: "Old caption".into(),
                is_final: true,
            }],
            provisional: Some(TranscriptSegment {
                utterance_id: 2,
                start_micros: 5_000_000,
                end_micros: 5_300_000,
                source_language: "en".into(),
                text: "New caption".into(),
                is_final: false,
            }),
        };

        assert_eq!(
            recent_caption_lines(&snapshot, 4, 2_000_000),
            vec!["New caption"]
        );
        assert!(recent_caption_lines(&snapshot, 0, 2_000_000).is_empty());
    }
}
