use crossbeam_channel::{Receiver, Sender, TrySendError};

use crate::VisualFrame;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LatestFrameSend {
    pub replaced_frames: u64,
}

pub struct LatestFrameSender {
    sender: Sender<VisualFrame>,
    overflow_receiver: Receiver<VisualFrame>,
}

pub fn latest_frame_channel() -> (LatestFrameSender, Receiver<VisualFrame>) {
    let (sender, receiver) = crossbeam_channel::bounded(1);
    (
        LatestFrameSender {
            sender,
            overflow_receiver: receiver.clone(),
        },
        receiver,
    )
}

impl LatestFrameSender {
    pub fn send(&self, frame: VisualFrame) -> Result<LatestFrameSend, VisualFrame> {
        match self.sender.try_send(frame) {
            Ok(()) => Ok(LatestFrameSend { replaced_frames: 0 }),
            Err(TrySendError::Full(frame)) => {
                let mut replaced_frames = 0_u64;
                while self.overflow_receiver.try_recv().is_ok() {
                    replaced_frames = replaced_frames.saturating_add(1);
                }
                match self.sender.try_send(frame) {
                    Ok(()) => Ok(LatestFrameSend { replaced_frames }),
                    Err(TrySendError::Full(frame) | TrySendError::Disconnected(frame)) => {
                        Err(frame)
                    }
                }
            }
            Err(TrySendError::Disconnected(frame)) => Err(frame),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{PixelFormat, VisualFrame};

    use super::*;

    fn frame(sequence: u64) -> VisualFrame {
        VisualFrame::new(sequence, sequence, 1, 1, 4, PixelFormat::Bgra8, vec![0; 4])
            .expect("frame")
    }

    #[test]
    fn a_slow_consumer_receives_only_the_latest_frame() {
        let (sender, receiver) = latest_frame_channel();
        assert_eq!(sender.send(frame(1)).expect("first").replaced_frames, 0);
        assert_eq!(sender.send(frame(2)).expect("second").replaced_frames, 1);
        assert_eq!(receiver.recv().expect("latest").sequence, 2);
    }
}
