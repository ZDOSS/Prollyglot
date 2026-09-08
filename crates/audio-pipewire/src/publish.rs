use std::time::{Duration, Instant};

use crossbeam_channel::{Sender, TrySendError};
use prollyglot_audio_pipeline::{SignalActivity, normalize_interleaved};
use prollyglot_core::{CaptureError, CaptureEvent, CaptureState, NativeAudioFormat, SourceId};

pub(crate) struct Publisher {
    events: Sender<CaptureEvent>,
    source: SourceId,
    started: Instant,
    sequence: u64,
    discontinuity: bool,
    activity: SignalActivity,
    state: Option<CaptureState>,
    dropped: u64,
    reported_dropped: u64,
    pub disconnected: bool,
}

impl Publisher {
    pub fn new(events: Sender<CaptureEvent>, source: SourceId) -> Self {
        Self {
            events,
            source,
            started: Instant::now(),
            sequence: 0,
            discontinuity: true,
            activity: SignalActivity::new(Duration::from_secs(2), 0.0001),
            state: None,
            dropped: 0,
            reported_dropped: 0,
            disconnected: false,
        }
    }

    pub fn reopened(&mut self) {
        self.discontinuity = true;
        // Observe a nominal signal at the current session time to give the new
        // stream its own silence grace period without resetting capture time.
        self.activity.observe(self.started.elapsed(), 1.0);
        self.state = Some(CaptureState::Capturing);
    }

    pub fn tick(&mut self) {
        if let Some(state) = self.activity.tick(self.started.elapsed()) {
            self.state = Some(state);
        }
        if let Some(state) = self.state
            && self.send(CaptureEvent::State(state))
        {
            self.state = None;
        }
        if self.dropped != self.reported_dropped
            && self.send(CaptureEvent::FramesDropped {
                total: self.dropped,
            })
        {
            self.reported_dropped = self.dropped;
        }
    }

    pub fn send(&mut self, event: CaptureEvent) -> bool {
        match self.events.try_send(event) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => false,
            Err(TrySendError::Disconnected(_)) => {
                self.disconnected = true;
                false
            }
        }
    }

    pub fn frame(&mut self, bytes: &[u8], format: NativeAudioFormat) -> Result<(), CaptureError> {
        if bytes.is_empty() {
            return Ok(());
        }
        let elapsed = self.started.elapsed();
        let frame = normalize_interleaved(
            self.sequence,
            self.source.clone(),
            elapsed.as_micros().min(u128::from(u64::MAX)) as u64,
            format,
            bytes,
            false,
            self.discontinuity,
        )?;
        self.sequence = self.sequence.wrapping_add(1);
        if let Some(state) = self.activity.observe(elapsed, frame.peak) {
            self.state = Some(state);
        }
        self.tick();
        if self.send(CaptureEvent::Frame(frame)) {
            self.discontinuity = false;
        } else {
            self.dropped = self.dropped.saturating_add(1);
            self.discontinuity = true;
        }
        Ok(())
    }
}

/// Read the valid SPA chunk, including a wrapped offset. Bounds come from the
/// mapped PipeWire buffer, never from unchecked pointer arithmetic.
pub(crate) fn read_chunk(
    data: &[u8],
    offset: usize,
    size: usize,
    stride: i32,
    frame_size: usize,
) -> Result<Vec<u8>, CaptureError> {
    if size == 0 {
        return Ok(Vec::new());
    }
    if data.is_empty()
        || size > data.len()
        || !size.is_multiple_of(frame_size)
        || (stride != 0 && stride != frame_size as i32)
    {
        return Err(CaptureError::InvalidFormat(
            "PipeWire returned an invalid PCM chunk.".into(),
        ));
    }
    let offset = offset % data.len();
    let first = size.min(data.len() - offset);
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(&data[offset..offset + first]);
    bytes.extend_from_slice(&data[..size - first]);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use prollyglot_core::SampleFormat;

    #[test]
    fn validates_chunk_bounds_and_reads_wrapped_data() {
        assert_eq!(
            read_chunk(&[1, 2, 3, 4, 5, 6, 7, 8], 6, 4, 4, 4).unwrap(),
            [7, 8, 1, 2]
        );
        assert!(read_chunk(&[0; 8], 0, 12, 4, 4).is_err());
        assert!(read_chunk(&[0; 8], 0, 6, 4, 4).is_err());
        assert!(read_chunk(&[0; 8], 0, 4, 8, 4).is_err());
    }

    #[test]
    fn queue_overflow_and_reopen_preserve_time_and_mark_discontinuity() {
        let (tx, rx) = crossbeam_channel::bounded(4);
        let mut publisher = Publisher::new(tx, SourceId::new("fixture"));
        let format = NativeAudioFormat {
            sample_rate: 48000,
            channels: 1,
            sample_format: SampleFormat::F32,
        };
        let bytes = 0.5f32.to_le_bytes();
        publisher.frame(&bytes, format).unwrap();
        let CaptureEvent::Frame(first) = rx.recv().unwrap() else {
            panic!("expected frame")
        };
        for _ in 0..10 {
            publisher.frame(&bytes, format).unwrap();
        }
        rx.try_iter().for_each(drop);
        publisher.frame(&bytes, format).unwrap();
        let frames: Vec<_> = rx.try_iter().collect();
        assert!(
            frames
                .iter()
                .any(|e| matches!(e, CaptureEvent::FramesDropped { total } if *total > 0))
        );
        let newest = frames
            .iter()
            .find_map(|e| match e {
                CaptureEvent::Frame(f) => Some(f),
                _ => None,
            })
            .unwrap();
        assert!(newest.discontinuity && newest.sequence > first.sequence);
        publisher.reopened();
        publisher.frame(&bytes, format).unwrap();
        let recovered = rx
            .try_iter()
            .find_map(|e| match e {
                CaptureEvent::Frame(f) => Some(f),
                _ => None,
            })
            .unwrap();
        assert!(recovered.discontinuity && recovered.sequence > newest.sequence);
        assert!(recovered.captured_at_micros >= newest.captured_at_micros);
    }
}
