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
}
