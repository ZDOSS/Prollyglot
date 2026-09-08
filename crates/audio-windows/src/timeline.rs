use std::time::{Duration, Instant};

/// One clock and sequence for the complete capture session, including recovery.
pub(crate) struct CaptureTimeline {
    started_at: Instant,
    sequence: u64,
    discontinuity: bool,
}

impl CaptureTimeline {
    pub(crate) fn new() -> Self {
        Self {
            started_at: Instant::now(),
            sequence: 0,
            discontinuity: true,
        }
    }

    pub(crate) fn reopen(&mut self) {
        self.discontinuity = true;
    }

    pub(crate) fn next(&mut self, device_discontinuity: bool) -> (u64, u64, bool) {
        self.stamp(self.started_at.elapsed(), device_discontinuity)
    }

    fn stamp(&mut self, elapsed: Duration, device_discontinuity: bool) -> (u64, u64, bool) {
        let stamp = (
            self.sequence,
            elapsed.as_micros().min(u128::from(u64::MAX)) as u64,
            self.discontinuity || device_discontinuity,
        );
        self.sequence = self.sequence.wrapping_add(1);
        self.discontinuity = false;
        stamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_preserves_the_session_clock_and_resets_audio_processing_once() {
        let mut timeline = CaptureTimeline::new();
        assert_eq!(timeline.next(false).0, 0);
        assert_eq!(
            timeline.stamp(Duration::from_secs(60), false),
            (1, 60_000_000, false)
        );
        timeline.reopen();
        assert_eq!(
            timeline.stamp(Duration::from_millis(62_100), false),
            (2, 62_100_000, true)
        );
        assert_eq!(
            timeline.stamp(Duration::from_millis(62_200), false),
            (3, 62_200_000, false)
        );
        assert!(timeline.stamp(Duration::from_millis(62_300), true).2);
    }
}
