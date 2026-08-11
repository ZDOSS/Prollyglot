use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{OcrObservation, VisualRect};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStabilizerConfig {
    pub required_consecutive_frames: u8,
    pub maximum_missing_frames: u64,
    pub minimum_confidence: f32,
    pub minimum_overlap: f32,
    pub bounds_smoothing: f32,
}

impl Default for TextStabilizerConfig {
    fn default() -> Self {
        Self {
            required_consecutive_frames: 2,
            maximum_missing_frames: 1,
            minimum_confidence: 0.35,
            minimum_overlap: 0.25,
            bounds_smoothing: 0.4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StableTextRegion {
    pub track_id: u64,
    pub text_revision: u64,
    pub text: String,
    pub confidence: f32,
    pub language: Option<String>,
    pub script: Option<String>,
    pub bounds: VisualRect,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StabilizerUpdate {
    pub visible: Vec<StableTextRegion>,
    /// Only these regions need a new translation. Position-only changes do not.
    pub translation_requests: Vec<StableTextRegion>,
    pub removed_track_ids: Vec<u64>,
}

#[derive(Clone, Debug)]
struct Track {
    id: u64,
    bounds: VisualRect,
    candidate: OcrObservation,
    candidate_normalized: String,
    candidate_hits: u8,
    visible: Option<StableTextRegion>,
    last_seen_sequence: u64,
}

pub struct TextStabilizer {
    config: TextStabilizerConfig,
    next_track_id: u64,
    tracks: Vec<Track>,
}

impl TextStabilizer {
    pub fn new(config: TextStabilizerConfig) -> Self {
        Self {
            config,
            next_track_id: 1,
            tracks: Vec::new(),
        }
    }

    pub fn update(&mut self, sequence: u64, observations: Vec<OcrObservation>) -> StabilizerUpdate {
        let mut matched_tracks = HashSet::new();
        let mut translation_requests = Vec::new();
        for observation in observations
            .into_iter()
            .filter(|observation| observation.is_usable(self.config.minimum_confidence))
        {
            let best_match = self
                .tracks
                .iter()
                .enumerate()
                .filter(|(index, _)| !matched_tracks.contains(index))
                .map(|(index, track)| {
                    (
                        index,
                        track.bounds.intersection_over_union(observation.bounds),
                    )
                })
                .filter(|(_, overlap)| *overlap >= self.config.minimum_overlap)
                .max_by(|(_, left), (_, right)| left.total_cmp(right));

            if let Some((track_index, _)) = best_match {
                matched_tracks.insert(track_index);
                if let Some(promoted) = update_track(
                    &mut self.tracks[track_index],
                    sequence,
                    observation,
                    self.config,
                ) {
                    translation_requests.push(promoted);
                }
            } else {
                let normalized = observation.normalized_text();
                let id = self.next_track_id;
                self.next_track_id = self.next_track_id.saturating_add(1);
                let mut track = Track {
                    id,
                    bounds: observation.bounds,
                    candidate: observation,
                    candidate_normalized: normalized,
                    candidate_hits: 1,
                    visible: None,
                    last_seen_sequence: sequence,
                };
                if self.config.required_consecutive_frames <= 1 {
                    let promoted = promote_candidate(&mut track);
                    translation_requests.push(promoted);
                }
                self.tracks.push(track);
            }
        }

        let mut removed_track_ids = Vec::new();
        self.tracks.retain(|track| {
            let keep = sequence.saturating_sub(track.last_seen_sequence)
                <= self.config.maximum_missing_frames;
            if !keep && track.visible.is_some() {
                removed_track_ids.push(track.id);
            }
            keep
        });
        let mut visible: Vec<_> = self
            .tracks
            .iter()
            .filter_map(|track| track.visible.clone())
            .collect();
        visible.sort_by_key(|region| region.track_id);
        translation_requests.sort_by_key(|region| region.track_id);
        removed_track_ids.sort_unstable();
        StabilizerUpdate {
            visible,
            translation_requests,
            removed_track_ids,
        }
    }
}

fn update_track(
    track: &mut Track,
    sequence: u64,
    observation: OcrObservation,
    config: TextStabilizerConfig,
) -> Option<StableTextRegion> {
    let normalized = observation.normalized_text();
    let consecutive = sequence == track.last_seen_sequence.saturating_add(1);
    if normalized == track.candidate_normalized && consecutive {
        track.candidate_hits = track.candidate_hits.saturating_add(1);
    } else {
        track.candidate_hits = 1;
        track.candidate_normalized = normalized;
    }
    track.bounds = track
        .bounds
        .smoothed_toward(observation.bounds, config.bounds_smoothing);
    track.candidate = observation;
    track.last_seen_sequence = sequence;

    if track.candidate_hits < config.required_consecutive_frames {
        if let Some(visible) = &mut track.visible {
            visible.bounds = track.bounds;
        }
        return None;
    }

    let changed = track
        .visible
        .as_ref()
        .is_none_or(|visible| visible.text.to_lowercase() != track.candidate_normalized);
    if changed {
        Some(promote_candidate(track))
    } else {
        if let Some(visible) = &mut track.visible {
            visible.bounds = track.bounds;
            visible.confidence = track.candidate.confidence;
        }
        None
    }
}

fn promote_candidate(track: &mut Track) -> StableTextRegion {
    let text_revision = track
        .visible
        .as_ref()
        .map_or(1, |visible| visible.text_revision.saturating_add(1));
    let promoted = StableTextRegion {
        track_id: track.id,
        text_revision,
        text: track.candidate.text.trim().to_owned(),
        confidence: track.candidate.confidence,
        language: track.candidate.language.clone(),
        script: track.candidate.script.clone(),
        bounds: track.bounds,
    };
    track.visible = Some(promoted.clone());
    promoted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(text: &str, x: f32) -> OcrObservation {
        OcrObservation {
            text: text.into(),
            confidence: 0.9,
            language: Some("ja".into()),
            script: Some("Jpan".into()),
            bounds: VisualRect {
                x,
                y: 100.0,
                width: 200.0,
                height: 40.0,
            },
        }
    }

    #[test]
    fn requires_two_frames_before_requesting_translation() {
        let mut stabilizer = TextStabilizer::new(TextStabilizerConfig::default());
        let first = stabilizer.update(1, vec![observation("こんにちは", 10.0)]);
        assert!(first.visible.is_empty());
        let second = stabilizer.update(2, vec![observation("こんにちは", 12.0)]);
        assert_eq!(second.visible.len(), 1);
        assert_eq!(second.translation_requests.len(), 1);
    }

    #[test]
    fn one_frame_ocr_glitch_does_not_replace_visible_text() {
        let mut stabilizer = TextStabilizer::new(TextStabilizerConfig::default());
        stabilizer.update(1, vec![observation("ニュース", 10.0)]);
        stabilizer.update(2, vec![observation("ニュース", 10.0)]);
        let glitch = stabilizer.update(3, vec![observation("ニュースス", 11.0)]);
        assert_eq!(glitch.visible[0].text, "ニュース");
        assert!(glitch.translation_requests.is_empty());
        let corrected = stabilizer.update(4, vec![observation("ニュース", 11.0)]);
        assert_eq!(corrected.visible[0].text, "ニュース");
        assert!(corrected.translation_requests.is_empty());
    }

    #[test]
    fn a_stable_text_change_reuses_position_and_requests_translation_once() {
        let mut stabilizer = TextStabilizer::new(TextStabilizerConfig::default());
        stabilizer.update(1, vec![observation("uno", 10.0)]);
        let initial = stabilizer.update(2, vec![observation("uno", 10.0)]);
        let id = initial.visible[0].track_id;
        stabilizer.update(3, vec![observation("dos", 12.0)]);
        let changed = stabilizer.update(4, vec![observation("dos", 14.0)]);
        assert_eq!(changed.visible[0].track_id, id);
        assert_eq!(changed.visible[0].text, "dos");
        assert_eq!(changed.translation_requests.len(), 1);
        assert_eq!(changed.translation_requests[0].text_revision, 2);
    }

    #[test]
    fn removed_regions_are_reported_without_retaining_text() {
        let mut stabilizer = TextStabilizer::new(TextStabilizerConfig::default());
        stabilizer.update(1, vec![observation("menu", 10.0)]);
        let stable = stabilizer.update(2, vec![observation("menu", 10.0)]);
        let id = stable.visible[0].track_id;
        assert!(
            stabilizer
                .update(3, Vec::new())
                .removed_track_ids
                .is_empty()
        );
        let removed = stabilizer.update(4, Vec::new());
        assert_eq!(removed.removed_track_ids, vec![id]);
        assert!(removed.visible.is_empty());
    }
}
