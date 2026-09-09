use std::{
    collections::HashMap,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

use crossbeam_channel::Sender;
use futures_util::StreamExt;
use serde::Serialize;
use zbus::{
    Connection, Proxy,
    zvariant::{DynamicType, OwnedFd, OwnedObjectPath, OwnedValue, Value},
};

use crate::{
    CaptureError, CaptureEvent, CaptureOptions, CaptureSession, PortalSource, StreamInfo, video,
};

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.ScreenCast";
const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const CLOSE_TIMEOUT: Duration = Duration::from_millis(400);
const POLL: Duration = Duration::from_millis(20);
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
type Dict = HashMap<String, OwnedValue>;

pub(crate) fn failed(error: impl std::fmt::Display) -> CaptureError {
    CaptureError::Failed(error.to_string())
}

fn token() -> String {
    format!("prollyglot_{}", NEXT_TOKEN.fetch_add(1, Ordering::Relaxed))
}

fn string(value: &str) -> OwnedValue {
    Value::from(value).try_to_owned().expect("string variant")
}

async fn cancelled(stop: &AtomicBool) {
    while !stop.load(Ordering::Acquire) {
        tokio::time::sleep(POLL).await;
    }
}

async fn call<T>(
    stop: &AtomicBool,
    operation: impl Future<Output = zbus::Result<T>>,
) -> Result<T, CaptureError> {
    tokio::select! {
        biased;
        _ = cancelled(stop) => Err(CaptureError::Cancelled),
        result = tokio::time::timeout(CALL_TIMEOUT, operation) => {
            result.map_err(|_| failed("The desktop portal did not respond."))?.map_err(failed)
        }
    }
}

async fn proxy(
    connection: &Connection,
    path: String,
    interface: &'static str,
) -> zbus::Result<Proxy<'static>> {
    Proxy::new_owned(connection.clone(), DESTINATION, path, interface).await
}

async fn close(proxy: &Proxy<'_>) {
    let _ = tokio::time::timeout(CLOSE_TIMEOUT, proxy.call::<_, _, ()>("Close", &())).await;
}

fn handle_path(connection: &Connection, kind: &str, token: &str) -> Result<String, CaptureError> {
    let sender = connection
        .unique_name()
        .ok_or_else(|| failed("The portal connection has no bus name."))?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    Ok(format!("{PATH}/{kind}/{sender}/{token}"))
}

/// Subscribe before the method call: fast portal replies can emit Response
/// before returning the request handle. Close the known request on every error,
/// including Stop while the method itself is still awaiting its reply.
async fn request<B: Serialize + DynamicType>(
    portal: &Proxy<'_>,
    stop: &AtomicBool,
    method: &str,
    handle_token: &str,
    body: &B,
    picker: bool,
) -> Result<Dict, CaptureError> {
    let path = handle_path(portal.connection(), "request", handle_token)?;
    let request = call(
        stop,
        proxy(
            portal.connection(),
            path.clone(),
            "org.freedesktop.portal.Request",
        ),
    )
    .await?;
    let mut responses = call(stop, request.receive_signal("Response")).await?;
    let result = async {
        let returned: OwnedObjectPath = call(stop, portal.call(method, body)).await?;
        if returned.as_str() != path {
            if let Ok(actual) = tokio::time::timeout(CLOSE_TIMEOUT, proxy(portal.connection(), returned.to_string(), "org.freedesktop.portal.Request")).await
                && let Ok(actual) = actual {
                close(&actual).await;
            }
            return Err(failed("The desktop portal returned an unexpected request handle."));
        }
        let response = async {
            let message = responses.next().await.ok_or(CaptureError::Closed)?;
            let (code, results): (u32, Dict) = message.body().deserialize().map_err(failed)?;
            match code {
                0 => Ok(results),
                1 => Err(CaptureError::Cancelled),
                _ => Err(failed(format!("The desktop rejected {method}."))),
            }
        };
        // The owner may leave the picker open indefinitely. Noninteractive
        // CreateSession/SelectSources must not leave a startup worker hanging.
        tokio::select! {
            biased;
            _ = cancelled(stop) => Err(CaptureError::Cancelled),
            result = response => result,
            _ = tokio::time::sleep(CALL_TIMEOUT), if !picker => Err(failed(format!("The desktop did not finish {method}."))),
        }
    }.await;
    if result.is_err() {
        close(&request).await;
    }
    result
}

#[derive(Clone, Debug)]
pub(crate) struct Target {
    pub node_id: u32,
    pub serial: Option<u64>,
}

fn selected_stream(
    mut response: Dict,
    source: PortalSource,
    version: u32,
) -> Result<(Target, StreamInfo), CaptureError> {
    let streams = response
        .remove("streams")
        .ok_or_else(|| failed("The portal returned no screen stream."))?;
    let mut streams = Vec::<(u32, Dict)>::try_from(streams).map_err(failed)?;
    if streams.len() != 1 {
        return Err(failed(
            "The portal must return exactly one selected source.",
        ));
    }
    let (node_id, mut properties) = streams.remove(0);
    let expected_type = match source {
        PortalSource::Monitor => 1,
        PortalSource::Window => 2,
    };
    if let Some(actual) = properties.remove("source_type")
        && u32::try_from(actual).map_err(failed)? != expected_type
    {
        return Err(failed(
            "The portal returned a different source type than requested.",
        ));
    }
    let serial = properties
        .remove("pipewire-serial")
        .map(u64::try_from)
        .transpose()
        .map_err(failed)?;
    if version >= 6 && serial.is_none() {
        return Err(failed(
            "The portal did not supply the screen stream's PipeWire serial.",
        ));
    }
    if node_id == u32::MAX || serial == Some(0) {
        return Err(failed("The portal returned an invalid stream identity."));
    }
    let logical_position = properties
        .remove("position")
        .map(<(i32, i32)>::try_from)
        .transpose()
        .map_err(failed)?;
    let logical_size = properties
        .remove("size")
        .map(<(i32, i32)>::try_from)
        .transpose()
        .map_err(failed)?
        .map(|(w, h)| {
            if w <= 0 || h <= 0 {
                return Err(failed("The portal returned invalid logical dimensions."));
            }
            Ok((w as u32, h as u32))
        })
        .transpose()?;
    Ok((
        Target { node_id, serial },
        StreamInfo {
            source,
            logical_position: if source == PortalSource::Monitor {
                logical_position
            } else {
                None
            },
            logical_size,
        },
    ))
}

async fn run_connected(
    connection: &Connection,
    options: CaptureOptions,
    stop: Arc<AtomicBool>,
    events: Sender<CaptureEvent>,
    frames: prollyglot_visual_pipeline::LatestFrameSender,
) -> Result<(), CaptureError> {
    let portal = call(&stop, proxy(connection, PATH.into(), INTERFACE)).await?;
    let version: u32 = call(&stop, portal.get_property("version")).await?;
    let mut owner_changed = call(&stop, portal.receive_owner_changed()).await?;
    let available: u32 = call(&stop, portal.get_property("AvailableSourceTypes")).await?;
    let source_type = match options.source {
        PortalSource::Monitor => 1u32,
        PortalSource::Window => 2u32,
    };
    if available & source_type == 0 {
        return Err(failed(
            "This desktop does not offer the requested screen source.",
        ));
    }
    let session_token = token();
    let session_path = handle_path(connection, "session", &session_token)?;
    let session = call(
        &stop,
        proxy(
            connection,
            session_path.clone(),
            "org.freedesktop.portal.Session",
        ),
    )
    .await?;
    // Listen even during creation/selection/start, not only after obtaining FD.
    let mut closed = call(&stop, session.receive_signal("Closed")).await?;
    let work = async {
        let create_token = token();
        let response = request(
            &portal,
            &stop,
            "CreateSession",
            &create_token,
            &(Dict::from([
                ("handle_token".into(), string(&create_token)),
                ("session_handle_token".into(), string(&session_token)),
            ]),),
            false,
        )
        .await?;
        // The documented session_handle wire type is a string, despite naming
        // an object path. Accept the object-path representation as well.
        let returned = response
            .get("session_handle")
            .and_then(|v| <&str>::try_from(v).ok());
        let path_returned = response
            .get("session_handle")
            .and_then(|v| <&zbus::zvariant::ObjectPath<'_>>::try_from(v).ok());
        if returned.or_else(|| path_returned.map(|p| p.as_str())) != Some(session_path.as_str()) {
            return Err(failed("The portal returned an unexpected session handle."));
        }
        let path = OwnedObjectPath::try_from(session_path.clone()).map_err(failed)?;
        let select_token = token();
        let mut selection = Dict::from([
            ("handle_token".into(), string(&select_token)),
            ("types".into(), source_type.into()),
            ("multiple".into(), false.into()),
        ]);
        if version >= 2 {
            let modes: u32 = call(&stop, portal.get_property("AvailableCursorModes")).await?;
            // No cursor bitmap needs to enter OCR. Metadata leaves the cursor
            // out of the video just like Hidden; accept Embedded as a fallback.
            let cursor = [1u32, 4, 2]
                .into_iter()
                .find(|mode| modes & mode != 0)
                .ok_or_else(|| failed("The desktop offers no supported cursor mode."))?;
            selection.insert("cursor_mode".into(), cursor.into());
        }
        if version >= 4 {
            selection.insert("persist_mode".into(), 0u32.into());
        }
        request(
            &portal,
            &stop,
            "SelectSources",
            &select_token,
            &(path.clone(), selection),
            false,
        )
        .await?;
        let start_token = token();
        let response = request(
            &portal,
            &stop,
            "Start",
            &start_token,
            &(
                path.clone(),
                options.parent_window,
                Dict::from([("handle_token".into(), string(&start_token))]),
            ),
            true,
        )
        .await?;
        let (target, info) = selected_stream(response, options.source, version)?;
        let fd: OwnedFd = call(
            &stop,
            portal.call("OpenPipeWireRemote", &(path, Dict::new())),
        )
        .await?;
        let _ = events.try_send(CaptureEvent::Selected(info));
        let video_stop = stop.clone();
        let worker = thread::Builder::new()
            .name("portal-video".into())
            .spawn(move || video::run(fd.into(), target, video_stop, events, frames))
            .map_err(failed)?;
        // Native PipeWire objects are thread-local. Poll their join status while
        // continuing to service DBus and the portal's Closed signal.
        while !worker.is_finished() && !stop.load(Ordering::Acquire) {
            tokio::time::sleep(POLL).await;
        }
        stop.store(true, Ordering::Release);
        worker
            .join()
            .map_err(|_| failed("The PipeWire screen worker panicked."))?
    };
    // If Closed wins, do not drop a live native worker future. A separate
    // cancellation flag stops it; await work so its buffers/FD are released.
    tokio::pin!(work);
    let result = tokio::select! {
        biased;
        _ = owner_changed.next() => {
            stop.store(true, Ordering::Release);
            let _ = work.await;
            Err(failed("The desktop screen-sharing service restarted or disconnected."))
        }
        _ = closed.next() => {
            stop.store(true, Ordering::Release);
            let _ = work.await;
            Err(CaptureError::Closed)
        }
        result = &mut work => result,
    };
    close(&session).await;
    result
}

pub(crate) fn start(options: CaptureOptions) -> Result<CaptureSession, CaptureError> {
    let stop = Arc::new(AtomicBool::new(false));
    let (frames_tx, frames) = prollyglot_visual_pipeline::latest_frame_channel();
    let (events_tx, events) = crossbeam_channel::bounded(16);
    let worker_stop = stop.clone();
    let drain = frames.clone();
    let event_drain = events.clone();
    let worker = thread::Builder::new()
        .name("screen-portal".into())
        .spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(failed)
                .and_then(|runtime| {
                    runtime.block_on(async {
                        // A dedicated connection ensures dropping it also tears down
                        // requests/sessions if a faulty portal ignores Close.
                        let connection = call(&worker_stop, Connection::session()).await?;
                        let result = run_connected(
                            &connection,
                            options,
                            worker_stop,
                            events_tx.clone(),
                            frames_tx,
                        )
                        .await;
                        let _ = tokio::time::timeout(CLOSE_TIMEOUT, connection.close()).await;
                        result
                    })
                });
            while drain.try_recv().is_ok() {}
            let terminal = match result {
                Ok(()) | Err(CaptureError::Closed) => CaptureEvent::Closed,
                Err(CaptureError::Cancelled) => CaptureEvent::Cancelled,
                Err(error) => CaptureEvent::Failed(error.to_string()),
            };
            // Make room for terminal state even if the consumer stalled on repeated
            // format changes. Frame buffers have their own latest-only channel.
            while events_tx.is_full() {
                let _ = event_drain.try_recv();
            }
            let _ = events_tx.try_send(terminal);
        })
        .map_err(failed)?;
    Ok(CaptureSession {
        frames,
        events,
        stop,
        worker: Some(worker),
    })
}
