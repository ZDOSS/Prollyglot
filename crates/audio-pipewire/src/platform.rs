use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    os::unix::fs::MetadataExt,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crossbeam_channel::Sender;
use pipewire::{self as pw, properties::properties, spa, types::ObjectType};
use prollyglot_core::{
    CaptureError, CaptureEvent, CaptureRecovery, CaptureRecoveryKind, CaptureSelection,
    CaptureSession, CaptureState, NativeAudioFormat, ResolvedCaptureSelection, SampleFormat,
    SourceSnapshot,
};

use crate::{
    application::ApplicationMonitor,
    graph::{ApplicationNode, Client, Graph, Port, Sink},
    identity::resolve_process,
    publish::{Publisher, read_chunk},
};

const POLL: Duration = Duration::from_millis(50);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

fn error(context: &str, error: impl std::fmt::Display) -> CaptureError {
    CaptureError::Worker(format!("{context}: {error}"))
}

// Listeners must be destroyed before the proxies they reference. Rc variants
// retain the core/context/loop in the same order required by PipeWire.
struct MetadataBinding {
    _listener: pw::metadata::MetadataListener,
    _metadata: pw::metadata::Metadata,
    global: pw::registry::GlobalObject<pw::properties::PropertiesBox>,
}

impl MetadataBinding {
    fn new(
        registry: &pw::registry::Registry,
        global: pw::registry::GlobalObject<pw::properties::PropertiesBox>,
        graph: Rc<RefCell<Graph>>,
    ) -> Result<Self, CaptureError> {
        let proxy = registry
            .bind::<pw::metadata::Metadata, _>(&global)
            .map_err(|e| error("Read PipeWire default output", e))?;
        let listener = proxy
            .add_listener_local()
            .property(move |subject, key, _, value| {
                if subject == 0 && (key == Some("default.audio.sink") || key.is_none()) {
                    graph.borrow_mut().default_sink = value
                        .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
                        .and_then(|v| v.get("name")?.as_str().map(str::to_owned));
                }
                0
            })
            .register();
        Ok(Self {
            _listener: listener,
            _metadata: proxy,
            global,
        })
    }
}

pub(crate) struct Connection {
    _registry_listener: pw::registry::Listener,
    _objects: Rc<RefCell<HashMap<u32, Box<dyn Any>>>>,
    _metadata: Rc<RefCell<HashMap<u32, MetadataBinding>>>,
    _core_listener: pw::core::Listener,
    pub(crate) graph: Rc<RefCell<Graph>>,
    _registry: pw::registry::RegistryRc,
    pub(crate) core: pw::core::CoreRc,
    main_loop: pw::main_loop::MainLoopRc,
    failure: Rc<RefCell<Option<String>>>,
}

impl Connection {
    fn new(stop: &AtomicBool) -> Result<Self, CaptureError> {
        // pipewire-rs initializes the process library once internally. Do not
        // deinitialize it while other source queries/capture workers exist.
        pw::init();
        let main_loop =
            pw::main_loop::MainLoopRc::new(None).map_err(|e| error("Create PipeWire loop", e))?;
        let context = pw::context::ContextRc::new(
            &main_loop,
            Some(properties! {
                "application.name" => "Prollyglot",
                "application.id" => "com.prollyglot.desktop",
            }),
        )
        .map_err(|e| error("Create PipeWire context", e))?;
        let core = context.connect_rc(None).map_err(|e| {
            CaptureError::SourceUnavailable(format!(
                "PipeWire is unavailable. Start PipeWire and WirePlumber in your desktop session. ({e})"
            ))
        })?;
        let registry = core
            .get_registry_rc()
            .map_err(|e| error("Read PipeWire registry", e))?;
        let graph = Rc::new(RefCell::new(Graph::default()));
        let metadata = Rc::new(RefCell::new(HashMap::new()));
        // Each tuple stores its listener before its proxy so it unregisters
        // first. The opaque boxes only retain native object lifetimes.
        let objects: Rc<RefCell<HashMap<u32, Box<dyn Any>>>> = Rc::default();
        let failure = Rc::new(RefCell::new(None));
        let error_slot = failure.clone();
        let server_graph = graph.clone();
        let core_listener = core
            .add_listener_local()
            .info(move |info| server_graph.borrow_mut().server_cookie = Some(info.cookie()))
            .error(move |id, _, result, message| {
                if id == pw::core::PW_ID_CORE {
                    *error_slot.borrow_mut() =
                        Some(format!("PipeWire connection failed ({result}): {message}"));
                }
            })
            .register();
        let added_graph = graph.clone();
        let removed_graph = graph.clone();
        let added_metadata = metadata.clone();
        let removed_metadata = metadata.clone();
        let weak_registry = registry.downgrade();
        let added_objects = objects.clone();
        let removed_objects = objects.clone();
        let registry_listener = registry
            .add_listener_local()
            .global(move |global| {
                let Some(props) = global.props else { return };
                if global.type_ == ObjectType::Node
                    && props.get("media.class") == Some("Audio/Sink")
                {
                    if let (Some(name), Some(serial)) =
                        (props.get("node.name"), props.get("object.serial"))
                    {
                        added_graph.borrow_mut().sinks.insert(
                            global.id,
                            Sink {
                                name: name.into(),
                                serial: serial.into(),
                                description: props
                                    .get("node.description")
                                    .or_else(|| props.get("node.nick"))
                                    .unwrap_or(name)
                                    .into(),
                            },
                        );
                    }
                } else if global.type_ == ObjectType::Node
                    && props.get("media.class") == Some("Stream/Output/Audio")
                {
                    let Some(registry) = weak_registry.upgrade() else {
                        return;
                    };
                    let Ok(node) = registry.bind::<pw::node::Node, _>(global) else {
                        return;
                    };
                    let id = global.id;
                    let graph = added_graph.clone();
                    let listener = node
                        .add_listener_local()
                        .info(move |info| {
                            if let Some(props) = info.props()
                                && let Some(client) =
                                    props.get("client.id").and_then(|v| v.parse().ok())
                            {
                                graph.borrow_mut().applications.insert(
                                    id,
                                    ApplicationNode {
                                        client,
                                        application_id: property(props, "application.id"),
                                        name: property(props, "application.name"),
                                    },
                                );
                            }
                        })
                        .register();
                    added_objects
                        .borrow_mut()
                        .insert(id, Box::new((listener, node)));
                } else if global.type_ == ObjectType::Client {
                    let Some(registry) = weak_registry.upgrade() else {
                        return;
                    };
                    let Ok(client) = registry.bind::<pw::client::Client, _>(global) else {
                        return;
                    };
                    let id = global.id;
                    let Some(serial) = property(props, "object.serial") else {
                        return;
                    };
                    let graph = added_graph.clone();
                    let listener = client
                        .add_listener_local()
                        .info(move |info| {
                            if let Some(props) = info.props() {
                                let uid = props
                                    .get("pipewire.sec.uid")
                                    .and_then(|v| v.parse().ok())
                                    .or_else(|| {
                                        std::fs::metadata("/proc/self").ok().map(|m| m.uid())
                                    });
                                // PulseAudio proxy clients may have a different
                                // authenticated peer PID; use their application PID
                                // only when readable and owned by the same user.
                                let pid = props
                                    .get("application.process.id")
                                    .or_else(|| props.get("pipewire.sec.pid"))
                                    .and_then(|v| v.parse().ok());
                                graph.borrow_mut().clients.insert(
                                    id,
                                    Client {
                                        serial: serial.clone(),
                                        application_id: property(props, "pipewire.sec.app-id")
                                            .or_else(|| property(props, "application.id")),
                                        name: property(props, "application.name"),
                                        process: pid
                                            .zip(uid)
                                            .and_then(|(pid, uid)| resolve_process(pid, uid)),
                                    },
                                );
                            }
                        })
                        .register();
                    added_objects
                        .borrow_mut()
                        .insert(id, Box::new((listener, client)));
                } else if global.type_ == ObjectType::Port {
                    if let (Some(node), Some(serial), Some(name)) = (
                        props.get("node.id").and_then(|v| v.parse().ok()),
                        props.get("object.serial"),
                        props.get("port.name"),
                    ) {
                        added_graph.borrow_mut().ports.insert(
                            global.id,
                            Port {
                                node,
                                serial: serial.into(),
                                name: name.into(),
                                output: props.get("port.direction") == Some("out"),
                                audio: props.get("format.dsp") == Some("32 bit float mono audio"),
                            },
                        );
                    }
                } else if global.type_ == ObjectType::Metadata
                    && props.get("metadata.name") == Some("default")
                {
                    let Some(registry) = weak_registry.upgrade() else {
                        return;
                    };
                    if let Ok(binding) =
                        MetadataBinding::new(&registry, global.to_owned(), added_graph.clone())
                    {
                        added_metadata.borrow_mut().insert(global.id, binding);
                    }
                }
            })
            .global_remove(move |id| {
                removed_graph.borrow_mut().sinks.remove(&id);
                removed_graph.borrow_mut().clients.remove(&id);
                removed_graph.borrow_mut().applications.remove(&id);
                removed_graph.borrow_mut().ports.remove(&id);
                removed_objects.borrow_mut().remove(&id);
                if removed_metadata.borrow_mut().remove(&id).is_some() {
                    removed_graph.borrow_mut().default_sink = None;
                }
            })
            .register();
        let connection = Self {
            _registry_listener: registry_listener,
            _objects: objects,
            _metadata: metadata,
            _core_listener: core_listener,
            graph,
            _registry: registry,
            core,
            main_loop,
            failure,
        };
        // The first roundtrip discovers the metadata proxy; the second receives
        // its initial properties. Both have a deadline and observe cancellation.
        connection.sync(stop)?;
        connection.sync(stop)?;
        Ok(connection)
    }

    pub(crate) fn sync(&self, stop: &AtomicBool) -> Result<(), CaptureError> {
        let sequence = self
            .core
            .sync(0)
            .map_err(|e| error("Synchronize PipeWire sources", e))?;
        let done = Rc::new(Cell::new(false));
        let completed = done.clone();
        let _listener = self
            .core
            .add_listener_local()
            .done(move |id, seq| {
                if id == pw::core::PW_ID_CORE && seq == sequence {
                    completed.set(true);
                }
            })
            .register();
        let started = Instant::now();
        while !done.get() && !stop.load(Ordering::Acquire) {
            self.iterate()?;
            if started.elapsed() > CONNECT_TIMEOUT {
                return Err(error(
                    "Synchronize PipeWire sources",
                    "server response timed out",
                ));
            }
        }
        if stop.load(Ordering::Acquire) {
            return Err(error("PipeWire startup", "cancelled"));
        }
        Ok(())
    }

    fn refresh_default(&self, stop: &AtomicBool) -> Result<(), CaptureError> {
        // A long-lived metadata subscription can miss an effective-default
        // update during concurrent source enumeration. Re-read it periodically
        // on the same loop, which continues delivering PCM during the roundtrip.
        let globals: Vec<_> = self
            ._metadata
            .borrow()
            .values()
            .map(|binding| binding.global.to_owned())
            .collect();
        self.graph.borrow_mut().default_sink = None;
        for global in globals {
            let id = global.id;
            let binding = MetadataBinding::new(&self._registry, global, self.graph.clone())?;
            self._metadata.borrow_mut().insert(id, binding);
        }
        self.sync(stop)
    }

    fn iterate(&self) -> Result<(), CaptureError> {
        if self
            .main_loop
            .loop_()
            .iterate(pw::loop_::Timeout::Finite(POLL))
            < 0
        {
            return Err(error("Process PipeWire events", "connection closed"));
        }
        if let Some(message) = self.failure.borrow_mut().take() {
            return Err(CaptureError::Worker(message));
        }
        Ok(())
    }
}

fn property(props: &spa::utils::dict::DictRef, key: &str) -> Option<String> {
    props
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

pub(crate) fn source_snapshot() -> Result<SourceSnapshot, CaptureError> {
    let connection = Connection::new(&AtomicBool::new(false))?;
    let snapshot = connection.graph.borrow().snapshot();
    Ok(snapshot)
}

pub(crate) fn resolve_selection(
    selection: &CaptureSelection,
) -> Result<ResolvedCaptureSelection, CaptureError> {
    let connection = Connection::new(&AtomicBool::new(false))?;
    let display_name = match selection {
        CaptureSelection::Application { source_id } => {
            connection.graph.borrow().application(source_id)?.0
        }
        _ => connection.graph.borrow().select(selection)?.description,
    };
    Ok(ResolvedCaptureSelection {
        selection: selection.clone(),
        source_id: selection.source_id(),
        display_name,
    })
}

struct Session {
    selection: CaptureSelection,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl CaptureSession for Session {
    fn selection(&self) -> &CaptureSelection {
        &self.selection
    }
    fn stop(&mut self) -> Result<(), CaptureError> {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| error("PipeWire capture", "worker panicked"))?;
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub(crate) fn start_capture(
    selection: CaptureSelection,
    events: Sender<CaptureEvent>,
) -> Result<Box<dyn CaptureSession>, CaptureError> {
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker_selection = selection.clone();
    let (ready, result) = crossbeam_channel::bounded(1);
    let worker = thread::Builder::new()
        .name("pipewire-capture".into())
        .spawn(move || {
            run(worker_selection, events, worker_stop, ready);
        })
        .map_err(|e| error("Start PipeWire worker", e))?;
    let mut session = Session {
        selection,
        stop,
        worker: Some(worker),
    };
    match result.recv_timeout(Duration::from_secs(8)) {
        Ok(Ok(())) => Ok(Box::new(session)),
        failed => {
            session.stop()?;
            Err(match failed {
                Ok(Err(e)) => e,
                _ => error("PipeWire startup", "capture did not become ready"),
            })
        }
    }
}

struct StreamData {
    format: Option<NativeAudioFormat>,
    publisher: Rc<RefCell<Publisher>>,
    failure: Rc<RefCell<Option<String>>>,
    connected: Rc<Cell<bool>>,
}

struct ActiveStream {
    _listener: pw::stream::StreamListener<StreamData>,
    _stream: pw::stream::StreamRc,
    target: Sink,
    failure: Rc<RefCell<Option<String>>>,
    connected: Rc<Cell<bool>>,
    started: Instant,
}

impl ActiveStream {
    fn open(
        connection: &Connection,
        target: Sink,
        publisher: Rc<RefCell<Publisher>>,
    ) -> Result<Self, CaptureError> {
        let stream = pw::stream::StreamRc::new(connection.core.clone(), "Prollyglot output captions", properties! {
            "application.name" => "Prollyglot",
            "application.id" => "com.prollyglot.desktop",
            "media.type" => "Audio", "media.category" => "Capture", "media.role" => "Accessibility",
            "stream.capture.sink" => "true",
            "target.object" => target.serial.clone(),
            // Own reconnect/default following here. Session-manager fallback
            // must never silently switch a pinned output or capture a microphone.
            "node.dont-move" => "true", "node.dont-fallback" => "true", "node.dont-reconnect" => "true",
            "node.stream.restore-target" => "false",
            "audio.rate" => "48000", "audio.channels" => "1", "audio.position" => "[ MONO ]",
        }).map_err(|e| error("Create PipeWire capture stream", e))?;
        let failure = Rc::new(RefCell::new(None));
        let connected = Rc::new(Cell::new(false));
        let listener = stream
            .add_local_listener_with_user_data(StreamData {
                format: None,
                publisher,
                failure: failure.clone(),
                connected: connected.clone(),
            })
            .state_changed(|_, data, _, state| match state {
                pw::stream::StreamState::Paused | pw::stream::StreamState::Streaming => {
                    data.connected.set(true)
                }
                pw::stream::StreamState::Error(message) => {
                    *data.failure.borrow_mut() = Some(message);
                }
                pw::stream::StreamState::Unconnected if data.connected.get() => {
                    *data.failure.borrow_mut() = Some("capture stream disconnected".into());
                }
                _ => {}
            })
            .param_changed(|_, data, id, param| {
                if id != spa::param::ParamType::Format.as_raw() {
                    return;
                }
                data.format = None;
                let Some(param) = param else {
                    return;
                };
                let mut format = spa::param::audio::AudioInfoRaw::new();
                if format.parse(param).is_err()
                    || format.format() != spa::param::audio::AudioFormat::F32LE
                    || format.channels() == 0
                    || format.channels() > 32
                    || format.rate() == 0
                {
                    *data.failure.borrow_mut() =
                        Some("PipeWire did not negotiate interleaved floating-point PCM.".into());
                    return;
                }
                data.format = Some(NativeAudioFormat {
                    sample_rate: format.rate(),
                    channels: format.channels() as u16,
                    sample_format: SampleFormat::F32,
                });
            })
            .process(|stream, data| {
                let Some(format) = data.format else { return };
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let Some(mapped) = buffer.datas_mut().first_mut() else {
                    return;
                };
                let (offset, size, stride) = (
                    mapped.chunk().offset() as usize,
                    mapped.chunk().size() as usize,
                    mapped.chunk().stride(),
                );
                let Some(bytes) = mapped.data() else { return };
                let result = read_chunk(bytes, offset, size, stride, format.bytes_per_frame())
                    .and_then(|bytes| data.publisher.borrow_mut().frame(&bytes, format));
                if let Err(e) = result {
                    *data.failure.borrow_mut() = Some(e.to_string());
                }
            })
            .register()
            .map_err(|e| error("Listen to PipeWire capture", e))?;
        let mut format = spa::param::audio::AudioInfoRaw::new();
        format.set_format(spa::param::audio::AudioFormat::F32LE);
        format.set_rate(48000);
        format.set_channels(1);
        let mut position = [0; spa::param::audio::MAX_CHANNELS];
        position[0] = spa::sys::SPA_AUDIO_CHANNEL_MONO;
        format.set_position(position);
        let values = spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &spa::pod::Value::Object(spa::pod::Object {
                type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
                id: spa::param::ParamType::EnumFormat.as_raw(),
                properties: format.into(),
            }),
        )
        .map_err(|e| error("Build PipeWire PCM format", e))?
        .0
        .into_inner();
        let pod = spa::pod::Pod::from_bytes(&values)
            .ok_or_else(|| error("Build PipeWire PCM format", "invalid format pod"))?;
        stream
            .connect(
                spa::utils::Direction::Input,
                None,
                pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
                &mut [pod],
            )
            .map_err(|e| error("Connect PipeWire output monitor", e))?;
        // No RT_PROCESS: normalization and bounded channel publication happen
        // on this worker's loop, never on PipeWire's real-time graph thread.
        Ok(Self {
            _listener: listener,
            _stream: stream,
            target,
            failure,
            connected,
            started: Instant::now(),
        })
    }
}

fn run(
    selection: CaptureSelection,
    events: Sender<CaptureEvent>,
    stop: Arc<AtomicBool>,
    ready: Sender<Result<(), CaptureError>>,
) {
    let publisher = Rc::new(RefCell::new(Publisher::new(events, selection.source_id())));
    let mut ready = Some(ready);
    let mut last_recovery = None;
    while !stop.load(Ordering::Acquire) && !publisher.borrow().disconnected {
        let result = Connection::new(&stop).and_then(|connection| {
            if let CaptureSelection::Application { source_id } = &selection {
                return run_application(
                    &connection,
                    source_id,
                    &publisher,
                    &stop,
                    &mut ready,
                    &mut last_recovery,
                );
            }
            let mut active: Option<ActiveStream> = None;
            let mut default_checked = Instant::now();
            while !stop.load(Ordering::Acquire) && !publisher.borrow().disconnected {
                if matches!(selection, CaptureSelection::SystemDefault)
                    && default_checked.elapsed() >= Duration::from_secs(1)
                {
                    connection.refresh_default(&stop)?;
                    default_checked = Instant::now();
                }
                let target = connection.graph.borrow().select(&selection);
                match target {
                    Ok(target) => {
                        if active.as_ref().is_none_or(|stream| stream.target != target) {
                            let changing_default = active.is_some()
                                && matches!(selection, CaptureSelection::SystemDefault);
                            drop(active.take());
                            if changing_default {
                                report_recovery(
                                    &publisher,
                                    "The system default playback device changed.",
                                    CaptureRecoveryKind::DefaultPlaybackDeviceChanged,
                                    &mut last_recovery,
                                );
                            }
                            active =
                                Some(ActiveStream::open(&connection, target, publisher.clone())?);
                            publisher.borrow_mut().reopened();
                        }
                    }
                    Err(e) => {
                        drop(active.take());
                        if ready.is_some() {
                            return Err(e);
                        }
                        report_recovery(
                            &publisher,
                            &e.to_string(),
                            CaptureRecoveryKind::PlaybackDeviceUnavailable,
                            &mut last_recovery,
                        );
                    }
                }
                connection.iterate()?;
                if let Some(stream) = &active {
                    if let Some(message) = stream.failure.borrow_mut().take() {
                        return Err(CaptureError::Worker(message));
                    }
                    if stream.connected.get() {
                        if let Some(ready) = ready.take() {
                            let _ = ready.send(Ok(()));
                        }
                        last_recovery = None;
                    } else if stream.started.elapsed() > CONNECT_TIMEOUT {
                        return Err(error(
                            "Connect PipeWire output monitor",
                            "WirePlumber did not link the selected output",
                        ));
                    }
                    publisher.borrow_mut().tick();
                }
            }
            Ok(())
        });
        if stop.load(Ordering::Acquire) {
            break;
        }
        if let Err(e) = result {
            if let Some(ready) = ready.take() {
                let _ = ready.send(Err(e));
                break;
            }
            report_recovery(
                &publisher,
                &e.to_string(),
                if matches!(selection, CaptureSelection::Application { .. }) {
                    CaptureRecoveryKind::ApplicationUnavailable
                } else {
                    CaptureRecoveryKind::PlaybackDeviceUnavailable
                },
                &mut last_recovery,
            );
        }
        let retry = Instant::now();
        while retry.elapsed() < Duration::from_millis(500) && !stop.load(Ordering::Acquire) {
            thread::sleep(POLL);
        }
    }
    publisher
        .borrow_mut()
        .send(CaptureEvent::State(CaptureState::Stopped));
}

fn run_application(
    connection: &Connection,
    source_id: &prollyglot_core::SourceId,
    publisher: &Rc<RefCell<Publisher>>,
    stop: &AtomicBool,
    ready: &mut Option<Sender<Result<(), CaptureError>>>,
    last_recovery: &mut Option<String>,
) -> Result<(), CaptureError> {
    let mut active: Option<ApplicationMonitor> = None;
    let mut pending_since = None;
    while !stop.load(Ordering::Acquire) && !publisher.borrow().disconnected {
        let selected = connection.graph.borrow().application(source_id);
        match selected {
            Ok((_, targets)) if !targets.is_empty() => {
                if active.is_none() {
                    active = Some(ApplicationMonitor::new(connection, publisher.clone())?);
                    publisher.borrow_mut().reopened();
                }
                if let Some(monitor) = &mut active {
                    monitor.update(connection, &targets, stop)?;
                }
            }
            selected => {
                drop(active.take());
                pending_since = None;
                let error = selected.err().unwrap_or_else(|| {
                    CaptureError::SourceUnavailable(
                        "The selected application is not playing capturable audio.".into(),
                    )
                });
                if ready.is_some() {
                    return Err(error);
                }
                let kind = if matches!(error, CaptureError::AmbiguousSource(_)) {
                    CaptureRecoveryKind::ApplicationAmbiguous
                } else {
                    CaptureRecoveryKind::ApplicationUnavailable
                };
                report_recovery(publisher, &error.to_string(), kind, last_recovery);
            }
        }
        connection.iterate()?;
        if let Some(monitor) = &active {
            let failure = monitor.failure.borrow_mut().take();
            if let Some(message) = failure {
                return Err(CaptureError::Worker(message));
            }
            if monitor.ready() {
                pending_since = None;
                if let Some(ready) = ready.take() {
                    let _ = ready.send(Ok(()));
                }
                *last_recovery = None;
            } else if pending_since.get_or_insert_with(Instant::now).elapsed() > CONNECT_TIMEOUT {
                // A playback stream added to a running application gets its
                // own connection deadline, regardless of the session's age.
                return Err(error(
                    "Connect application audio",
                    "the playback streams did not become ready",
                ));
            }
            publisher.borrow_mut().tick();
        }
    }
    Ok(())
}

fn report_recovery(
    publisher: &Rc<RefCell<Publisher>>,
    reason: &str,
    kind: CaptureRecoveryKind,
    previous: &mut Option<String>,
) {
    if previous.as_deref() != Some(reason)
        && publisher
            .borrow_mut()
            .send(CaptureEvent::Recovery(CaptureRecovery {
                kind,
                message: format!(
                    "{reason} Prollyglot will retry the selected source automatically."
                ),
                retry_after_millis: 500,
            }))
    {
        *previous = Some(reason.into());
    }
}
