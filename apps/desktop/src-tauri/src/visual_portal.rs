//! Portal startup and bounded cropping, before pixels enter OCR.
use crate::{
    visual_capture::{PickedVisualSource, StartError, VisualCaptureEvent},
    visual_geometry::PortalGeometry,
};
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, TryRecvError, TrySendError};
use prollyglot_application_runtime::{CancellationToken, VisualCaptureSelection};
use prollyglot_visual_pipeline::{VisualFrame, latest_frame_channel};
use prollyglot_visual_pipewire::{
    CaptureEvent, CaptureOptions, CaptureSession, FrameGeometry, PortalSource, StreamInfo,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub struct PortalCapture {
    session: CaptureSession,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl PortalCapture {
    pub fn stop(&mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::Release);
        let capture = self.session.stop().map_err(|e| e.to_string());
        let worker = self.worker.take().map_or(Ok(()), |worker| {
            worker
                .join()
                .map_err(|_| "Screen region worker panicked.".to_string())
        });
        capture.and(worker)
    }
}
impl Drop for PortalCapture {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

type Started = (
    PortalCapture,
    PickedVisualSource,
    Receiver<VisualFrame>,
    Receiver<VisualCaptureEvent>,
);

pub fn start_capture(
    app: tauri::AppHandle,
    selection: VisualCaptureSelection,
    parent_window: String,
    cancellation: CancellationToken,
) -> Result<Started, StartError> {
    let region = matches!(selection, VisualCaptureSelection::PortalRegion);
    let source = if matches!(selection, VisualCaptureSelection::PortalWindow) {
        PortalSource::Window
    } else {
        PortalSource::Monitor
    };
    let session = prollyglot_visual_pipewire::start_capture(CaptureOptions {
        source,
        parent_window,
    })
    .map_err(|e| StartError::Failed(e.to_string()))?;
    // CaptureSession's Drop closes and joins even when the picker or preview
    // fails. Startup never leaves an unowned sharing session behind.
    let mut geometry = wait_for_geometry(&session.events, &cancellation)?;
    let first = loop {
        if cancellation.is_cancelled() {
            return Err(StartError::Cancelled);
        }
        check_startup_events(&session.events, geometry.frame)?;
        match session.frames.recv_timeout(Duration::from_millis(20)) {
            Ok(frame)
                if frame.width == geometry.frame.width && frame.height == geometry.frame.height =>
            {
                break frame;
            }
            Ok(_) => {
                return Err(StartError::Failed(
                    "The shared image changed size. Start again to select it.".into(),
                ));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(StartError::Cancelled),
        }
    };
    if region {
        geometry.region = Some(crate::visual_linux::select_region(
            &app,
            &first,
            &cancellation,
            &session.events,
            geometry.frame,
        )?);
    }
    check_startup_events(&session.events, geometry.frame)?;
    let source = picked_source(&geometry);
    let (sender, frames) = latest_frame_channel();
    let (events_tx, events) = crossbeam_channel::bounded(16);
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&stop);
    let native_events = session.events.clone();
    let native_frames = session.frames.clone();
    let event_sink = EventSink {
        sender: events_tx,
        overflow: events.clone(),
    };
    let worker = thread::Builder::new()
        .name("visual-portal-region".into())
        .spawn(move || {
            forward_frames(
                first,
                geometry,
                native_frames,
                native_events,
                sender,
                event_sink,
                worker_stop,
                cancellation,
            );
        })
        .map_err(|e| StartError::Failed(e.to_string()))?;
    Ok((
        PortalCapture {
            session,
            stop,
            worker: Some(worker),
        },
        source,
        frames,
        events,
    ))
}

struct EventSink {
    sender: Sender<VisualCaptureEvent>,
    overflow: Receiver<VisualCaptureEvent>,
}
impl EventSink {
    fn send(&self, event: VisualCaptureEvent) {
        if let Err(TrySendError::Full(event)) = self.sender.try_send(event) {
            while self.overflow.try_recv().is_ok() {}
            let _ = self.sender.try_send(event);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn forward_frames(
    first: VisualFrame,
    mut geometry: PortalGeometry,
    frames: Receiver<VisualFrame>,
    events: Receiver<CaptureEvent>,
    output: prollyglot_visual_pipeline::LatestFrameSender,
    sink: EventSink,
    stop: Arc<AtomicBool>,
    cancellation: CancellationToken,
) {
    let mut pending = Some(first);
    while !stop.load(Ordering::Acquire) && !cancellation.is_cancelled() {
        let mut frame = pending.take();
        let mut disconnected = false;
        if frame.is_none() {
            match frames.recv_timeout(Duration::from_millis(20)) {
                Ok(next) => frame = Some(next),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    disconnected = true;
                }
            }
        }
        while let Ok(newer) = frames.try_recv() {
            frame = Some(newer);
        }
        // Native metadata precedes its frame. Drain AFTER choosing the latest
        // image, including when no new image arrives (revocation/sparse input).
        while let Ok(event) = events.try_recv() {
            match event {
                CaptureEvent::FormatChanged(next) if next != geometry.frame => {
                    if geometry.region.is_some() {
                        sink.send(VisualCaptureEvent::Failed("The shared monitor changed size or orientation. Start again and redraw the region.".into()));
                        return;
                    }
                    geometry.frame = next;
                    geometry.stream.logical_position = None;
                    sink.send(VisualCaptureEvent::Started(picked_source(&geometry)));
                }
                CaptureEvent::Failed(message) => {
                    sink.send(VisualCaptureEvent::Failed(message));
                    return;
                }
                CaptureEvent::Closed | CaptureEvent::Cancelled => {
                    sink.send(VisualCaptureEvent::SourceClosed);
                    return;
                }
                _ => {}
            }
        }
        if disconnected {
            sink.send(VisualCaptureEvent::SourceClosed);
            return;
        }
        let Some(frame) = frame else {
            continue;
        };
        if frame.width != geometry.frame.width || frame.height != geometry.frame.height {
            // A metadata change can overtake the old pending image. Wait for
            // its matching new buffer; never reinterpret the old pixels.
            continue;
        }
        match crop_frame(frame, &geometry) {
            Ok(frame) => {
                if output.send(frame).is_err() {
                    return;
                }
            }
            Err(message) => {
                sink.send(VisualCaptureEvent::Failed(message));
                return;
            }
        }
    }
}

fn picked_source(geometry: &PortalGeometry) -> PickedVisualSource {
    let (width, height) = geometry
        .region
        .map(|r| (r.width, r.height))
        .unwrap_or((geometry.frame.width, geometry.frame.height));
    PickedVisualSource {
        label: if geometry.region.is_some() {
            "Shared monitor region"
        } else if geometry.stream.source == PortalSource::Window {
            "Shared window"
        } else {
            "Shared monitor"
        }
        .into(),
        x: 0,
        y: 0,
        width,
        height,
        portal: Some(geometry.clone()),
    }
}

fn crop_frame(frame: VisualFrame, geometry: &PortalGeometry) -> Result<VisualFrame, String> {
    if frame.width != geometry.frame.width || frame.height != geometry.frame.height {
        return Err("The shared image changed. Start again to choose a region.".into());
    }
    match geometry.region {
        Some(region) => frame.crop(region).map_err(|e| e.to_string()),
        None => Ok(frame),
    }
}

fn wait_for_geometry(
    events: &Receiver<CaptureEvent>,
    cancellation: &CancellationToken,
) -> Result<PortalGeometry, StartError> {
    let mut stream: Option<StreamInfo> = None;
    loop {
        if cancellation.is_cancelled() {
            return Err(StartError::Cancelled);
        }
        match events.recv_timeout(Duration::from_millis(20)) {
            Ok(CaptureEvent::Selected(selected)) => stream = Some(selected),
            Ok(CaptureEvent::FormatChanged(frame)) => {
                return stream
                    .map(|stream| PortalGeometry {
                        stream,
                        frame,
                        region: None,
                    })
                    .ok_or_else(|| {
                        StartError::Failed("The desktop did not identify the shared source.".into())
                    });
            }
            Ok(CaptureEvent::Closed | CaptureEvent::Cancelled)
            | Err(RecvTimeoutError::Disconnected) => return Err(StartError::Cancelled),
            Ok(CaptureEvent::Failed(message)) => return Err(StartError::Failed(message)),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

pub fn check_startup_events(
    events: &Receiver<CaptureEvent>,
    geometry: FrameGeometry,
) -> Result<(), StartError> {
    loop {
        let event = match events.try_recv() {
            Ok(event) => event,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => return Err(StartError::Cancelled),
        };
        match event {
            CaptureEvent::Closed | CaptureEvent::Cancelled => return Err(StartError::Cancelled),
            CaptureEvent::Failed(message) => return Err(StartError::Failed(message)),
            CaptureEvent::FormatChanged(next) if next != geometry => {
                return Err(StartError::Failed(
                    "The shared image changed size or orientation. Start again to choose a region."
                        .into(),
                ));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prollyglot_visual_pipeline::{PixelFormat, PixelRect};

    fn token() -> CancellationToken {
        use prollyglot_application_runtime::{
            SessionMode, SessionSource, SessionSourceKind, SessionSupervisor, StartSessionRequest,
        };
        SessionSupervisor::default()
            .start(StartSessionRequest {
                mode: SessionMode::VisualTranslation,
                source: SessionSource::new("portal", SessionSourceKind::Region, "Region"),
            })
            .unwrap()
            .cancellation
    }

    #[test]
    fn sparse_capture_failure_is_delivered_without_another_image() {
        let (frames_tx, frames) = crossbeam_channel::bounded(1);
        let (events_tx, events) = crossbeam_channel::bounded(1);
        let (sender, output) = latest_frame_channel();
        let (sink_tx, sink_rx) = crossbeam_channel::bounded(1);
        let sink = EventSink {
            sender: sink_tx,
            overflow: sink_rx.clone(),
        };
        let geometry = PortalGeometry {
            stream: StreamInfo {
                source: PortalSource::Monitor,
                logical_position: None,
                logical_size: None,
            },
            frame: FrameGeometry {
                raw_width: 2,
                raw_height: 2,
                width: 2,
                height: 2,
                crop: PixelRect::full(2, 2),
                transform: 0,
            },
            region: Some(PixelRect::full(2, 2)),
        };
        let first = VisualFrame::new(1, 1, 2, 2, 8, PixelFormat::Bgra8, vec![255; 16]).unwrap();
        let worker = thread::spawn(move || {
            forward_frames(
                first,
                geometry,
                frames,
                events,
                sender,
                sink,
                Arc::new(AtomicBool::new(false)),
                token(),
            )
        });
        output.recv_timeout(Duration::from_secs(1)).unwrap();
        events_tx
            .send(CaptureEvent::Failed("sharing revoked".into()))
            .unwrap();
        // Closing the pixel channel must not downgrade an already queued error
        // to an ordinary source closure, or wait for a new frame to report it.
        drop(frames_tx);
        assert!(
            matches!(sink_rx.recv_timeout(Duration::from_secs(1)).unwrap(), VisualCaptureEvent::Failed(message) if message == "sharing revoked")
        );
        worker.join().unwrap();
        assert!(output.try_recv().is_err());
    }

    #[test]
    fn terminal_metadata_survives_a_stalled_consumer_and_cancelled_start() {
        let (sender, receiver) = crossbeam_channel::bounded(1);
        let sink = EventSink {
            sender,
            overflow: receiver.clone(),
        };
        sink.send(VisualCaptureEvent::SourceClosed);
        sink.send(VisualCaptureEvent::Failed("capture ended".into()));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            VisualCaptureEvent::Failed(_)
        ));
        let cancellation = token();
        cancellation.cancel();
        let (_sender, receiver) = crossbeam_channel::bounded(1);
        assert!(matches!(
            wait_for_geometry(&receiver, &cancellation),
            Err(StartError::Cancelled)
        ));
    }
    #[test]
    fn region_crop_removes_unselected_pixels_and_preserves_capture_evidence() {
        let geometry = PortalGeometry {
            stream: StreamInfo {
                source: PortalSource::Monitor,
                logical_position: None,
                logical_size: None,
            },
            frame: FrameGeometry {
                raw_width: 4,
                raw_height: 2,
                width: 4,
                height: 2,
                crop: PixelRect::full(4, 2),
                transform: 0,
            },
            region: Some(PixelRect {
                x: 2,
                y: 0,
                width: 2,
                height: 2,
            }),
        };
        let frame =
            VisualFrame::new(17, 1234, 4, 2, 16, PixelFormat::Bgra8, (0..32).collect()).unwrap();
        let cropped = crop_frame(frame, &geometry).unwrap();
        assert_eq!(
            (
                cropped.width,
                cropped.height,
                cropped.sequence,
                cropped.captured_at_micros
            ),
            (2, 2, 17, 1234)
        );
        assert_eq!(
            cropped.pixels(),
            &[8, 9, 10, 11, 12, 13, 14, 15, 24, 25, 26, 27, 28, 29, 30, 31]
        );
        assert!(
            crop_frame(cropped, &geometry).is_err(),
            "changed dimensions cannot reuse a crop"
        );
        let (tx, rx) = crossbeam_channel::bounded(1);
        tx.send(CaptureEvent::FormatChanged(FrameGeometry {
            transform: 2,
            ..geometry.frame
        }))
        .unwrap();
        assert!(
            check_startup_events(&rx, geometry.frame).is_err(),
            "same-size rotation invalidates selection"
        );
    }
}
