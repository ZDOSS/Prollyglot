//! One non-realtime PipeWire filter mixes an application's playback ports on
//! the graph clock. Existing application-to-output links are never changed.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ffi::{CStr, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr::{self, NonNull},
    rc::Rc,
    sync::atomic::AtomicBool,
};

use pipewire::{self as pw, properties::properties, spa};
use prollyglot_core::CaptureError;

use crate::{
    graph::MonitorPort,
    platform::Connection,
    publish::{Publisher, read_chunk},
};

struct CaptureLink {
    _listener: pw::link::LinkListener,
    _link: pw::link::Link,
    ready: Rc<Cell<bool>>,
}

struct InputPort {
    target: MonitorPort,
    data: NonNull<c_void>,
    link: Option<CaptureLink>,
}

struct FilterBuffer<'a> {
    buffer: NonNull<pw::sys::pw_buffer>,
    port: &'a InputPort,
}

impl FilterBuffer<'_> {
    fn samples(&self) -> Result<Vec<u8>, String> {
        // SAFETY: dequeue transfers access to this filter-owned buffer until
        // Drop queues it again. Check pointers/counts before reading metadata,
        // and respect mapped size, chunk offset, flags and valid byte count.
        let buffer = unsafe { self.buffer.as_ref().buffer.as_ref() }
            .ok_or("Missing application audio buffer.")?;
        if buffer.n_datas != 1 {
            return Err("Invalid application audio buffer planes.".into());
        }
        let data = unsafe { buffer.datas.as_ref() }.ok_or("Missing application audio plane.")?;
        let chunk = unsafe { data.chunk.as_ref() }.ok_or("Missing application audio chunk.")?;
        if chunk.flags & spa::sys::SPA_CHUNK_FLAG_CORRUPTED as i32 != 0 {
            return Err("PipeWire reported a corrupted application audio chunk.".into());
        }
        if chunk.size == 0 || chunk.flags & spa::sys::SPA_CHUNK_FLAG_EMPTY as i32 != 0 {
            return Ok(Vec::new());
        }
        if data.data.is_null() || chunk.size > 65536 * 4 {
            return Err("Application audio is unavailable in a mapped PCM buffer.".into());
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(data.data.cast::<u8>(), data.maxsize as usize) };
        read_chunk(
            bytes,
            chunk.offset as usize,
            chunk.size as usize,
            chunk.stride,
            4,
        )
        .map_err(|e| e.to_string())
    }
}

impl Drop for FilterBuffer<'_> {
    fn drop(&mut self) {
        // SAFETY: the borrowed InputPort outlives the buffer, and only this
        // guard queues the successfully dequeued native buffer.
        unsafe {
            pw::sys::pw_filter_queue_buffer(self.port.data.as_ptr(), self.buffer.as_ptr());
        }
    }
}

struct FilterData {
    inputs: RefCell<BTreeMap<u32, InputPort>>,
    publisher: Rc<RefCell<Publisher>>,
    failure: Rc<RefCell<Option<String>>>,
    connected: Rc<Cell<bool>>,
    previous: Cell<Option<(u32, u64)>>,
}

impl FilterData {
    fn process(&self, position: &spa::sys::spa_io_position) -> Result<(), String> {
        let clock = &position.clock;
        if clock.duration == 0
            || clock.duration > 65536
            || clock.rate.num != 1
            || clock.rate.denom == 0
        {
            return Err("PipeWire returned an invalid application audio clock.".into());
        }
        let inputs = self.inputs.borrow();
        if inputs.is_empty() {
            return Ok(());
        }
        let rate = clock.rate.denom;
        let count = clock.duration as usize;
        let mut samples = vec![0.0; count];
        for input in inputs.values() {
            // SAFETY: this input port belongs to the live filter. Retain the
            // buffer until its bounded, valid PCM chunk has been copied.
            let Some(buffer) =
                NonNull::new(unsafe { pw::sys::pw_filter_dequeue_buffer(input.data.as_ptr()) })
            else {
                continue;
            };
            let bytes = FilterBuffer {
                buffer,
                port: input,
            }
            .samples()?;
            for (bytes, mixed) in bytes.chunks_exact(4).zip(&mut samples) {
                let sample = f32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                if sample.is_finite() {
                    *mixed += sample.clamp(-1.0, 1.0) * input.target.gain;
                }
            }
        }
        let mut publisher = self.publisher.borrow_mut();
        if self
            .previous
            .get()
            .is_some_and(|(old_rate, expected)| old_rate != rate || expected != clock.position)
        {
            publisher.mark_discontinuity();
        }
        self.previous
            .set(Some((rate, clock.position.saturating_add(clock.duration))));
        publisher.mono(samples, rate);
        Ok(())
    }
}

unsafe extern "C" fn process(data: *mut c_void, position: *mut spa::sys::spa_io_position) {
    // SAFETY: listener data is a stable Box kept alive until the filter is
    // destroyed. RT_PROCESS is deliberately absent; callbacks and control
    // operations run on the same capture worker loop.
    let data = unsafe { &*data.cast::<FilterData>() };
    let result = catch_unwind(AssertUnwindSafe(|| {
        let position =
            unsafe { position.as_ref() }.ok_or("PipeWire audio clock is unavailable.")?;
        data.process(position)
    }));
    let message = match result {
        Ok(Ok(())) => return,
        Ok(Err(message)) => message,
        Err(_) => "Application audio processing failed.".into(),
    };
    if let Ok(mut failure) = data.failure.try_borrow_mut() {
        *failure = Some(message);
    }
}

unsafe extern "C" fn state_changed(
    data: *mut c_void,
    _: pw::sys::pw_filter_state,
    state: pw::sys::pw_filter_state,
    error: *const std::ffi::c_char,
) {
    // SAFETY: same stable listener data and worker-thread lifetime as process.
    let data = unsafe { &*data.cast::<FilterData>() };
    match state {
        pw::sys::pw_filter_state_PW_FILTER_STATE_PAUSED
        | pw::sys::pw_filter_state_PW_FILTER_STATE_STREAMING => data.connected.set(true),
        pw::sys::pw_filter_state_PW_FILTER_STATE_ERROR => {
            let message = if error.is_null() {
                "Application audio monitor failed.".into()
            } else {
                unsafe { CStr::from_ptr(error) }
                    .to_string_lossy()
                    .into_owned()
            };
            if let Ok(mut failure) = data.failure.try_borrow_mut() {
                *failure = Some(message);
            }
        }
        _ => data.connected.set(false),
    }
}

pub(crate) struct ApplicationMonitor {
    filter: NonNull<pw::sys::pw_filter>,
    // Keep the core, callbacks, hook and callback data alive until Drop destroys
    // the filter. None of these objects can cross a thread boundary.
    _core: pw::core::CoreRc,
    _events: Box<pw::sys::pw_filter_events>,
    _hook: Box<spa::sys::spa_hook>,
    data: Box<FilterData>,
    pub failure: Rc<RefCell<Option<String>>>,
}

impl ApplicationMonitor {
    pub fn new(
        connection: &Connection,
        publisher: Rc<RefCell<Publisher>>,
    ) -> Result<Self, CaptureError> {
        let failure = Rc::new(RefCell::new(None));
        let connected = Rc::new(Cell::new(false));
        let mut data = Box::new(FilterData {
            inputs: RefCell::new(BTreeMap::new()),
            publisher,
            failure: failure.clone(),
            connected: connected.clone(),
            previous: Cell::new(None),
        });
        // Zero initializes optional C callbacks and the unregistered SPA hook.
        let mut events: Box<pw::sys::pw_filter_events> = Box::new(unsafe { std::mem::zeroed() });
        events.version = pw::sys::PW_VERSION_FILTER_EVENTS;
        events.process = Some(process);
        events.state_changed = Some(state_changed);
        let mut hook: Box<spa::sys::spa_hook> = Box::new(unsafe { std::mem::zeroed() });
        let props = properties! {
            "application.name" => "Prollyglot", "application.id" => "com.prollyglot.desktop",
            "node.name" => "prollyglot-application-captions", "media.type" => "Audio",
            "media.category" => "Capture", "media.role" => "Accessibility",
            "media.class" => "Stream/Input/Audio", "node.autoconnect" => "false",
            "node.passive" => "true", "stream.monitor" => "true",
            "node.dont-move" => "true", "node.dont-fallback" => "true",
        };
        // SAFETY: the live core is retained below. PipeWire takes ownership of
        // the properties. Destruction is owned exactly once by this wrapper.
        let filter = NonNull::new(unsafe {
            pw::sys::pw_filter_new(
                connection.core.as_raw_ptr(),
                c"Prollyglot application captions".as_ptr(),
                props.into_raw(),
            )
        })
        .ok_or_else(|| {
            CaptureError::Worker("Could not create the application audio monitor.".into())
        })?;
        unsafe {
            pw::sys::pw_filter_add_listener(
                filter.as_ptr(),
                &mut *hook,
                &*events,
                (&mut *data as *mut FilterData).cast(),
            );
        }
        let monitor = Self {
            filter,
            _core: connection.core.clone(),
            _events: events,
            _hook: hook,
            data,
            failure,
        };
        // NONE means the process callback executes on this worker, never the
        // realtime graph thread. No driver or virtual playback device is added.
        let result = unsafe {
            pw::sys::pw_filter_connect(
                filter.as_ptr(),
                pw::sys::pw_filter_flags_PW_FILTER_FLAG_NONE,
                ptr::null_mut(),
                0,
            )
        };
        if result < 0 {
            return Err(CaptureError::Worker(format!(
                "Could not connect application audio ({result})."
            )));
        }
        Ok(monitor)
    }

    pub fn update(
        &mut self,
        connection: &Connection,
        targets: &[MonitorPort],
        stop: &AtomicBool,
    ) -> Result<(), CaptureError> {
        let remove: Vec<_> = self
            .data
            .inputs
            .borrow()
            .iter()
            .filter_map(|(id, input)| {
                (!targets
                    .iter()
                    .any(|target| target.id == *id && target.serial == input.target.serial))
                .then_some(*id)
            })
            .collect();
        for id in remove {
            if let Some(mut input) = self.data.inputs.borrow_mut().remove(&id) {
                drop(input.link.take());
                // SAFETY: the removed port belongs to this live filter. No
                // callback can run concurrently with this control operation.
                unsafe {
                    pw::sys::pw_filter_remove_port(input.data.as_ptr());
                }
            }
        }
        let mut added = false;
        for target in targets {
            if let Some(input) = self.data.inputs.borrow_mut().get_mut(&target.id) {
                input.target.gain = target.gain;
                continue;
            }
            let props = properties! {
                "format.dsp" => "32 bit float mono audio",
                "port.name" => format!("capture_{}", target.serial),
            };
            // SAFETY: PipeWire owns the new port and properties. No Rust value
            // is stored in the one-byte C user-data allocation used as a handle.
            let port = NonNull::new(unsafe {
                pw::sys::pw_filter_add_port(
                    self.filter.as_ptr(),
                    spa::sys::SPA_DIRECTION_INPUT,
                    pw::sys::pw_filter_port_flags_PW_FILTER_PORT_FLAG_MAP_BUFFERS,
                    1,
                    props.into_raw(),
                    ptr::null_mut(),
                    0,
                )
            })
            .ok_or_else(|| {
                CaptureError::Worker("Could not add an application audio channel.".into())
            })?;
            self.data.inputs.borrow_mut().insert(
                target.id,
                InputPort {
                    target: target.clone(),
                    data: port,
                    link: None,
                },
            );
            added = true;
        }
        if added {
            connection.sync(stop)?;
        }
        // Port globals arrive asynchronously. Resolve only ports on our own
        // node; never bind a similarly named port belonging to another client.
        let node = unsafe { pw::sys::pw_filter_get_node_id(self.filter.as_ptr()) };
        for input in self
            .data
            .inputs
            .borrow_mut()
            .values_mut()
            .filter(|input| input.link.is_none())
        {
            let name = format!("capture_{}", input.target.serial);
            let id = connection
                .graph
                .borrow()
                .ports
                .iter()
                .find_map(|(id, port)| {
                    (port.node == node && !port.output && port.name == name).then_some(*id)
                });
            if let Some(id) = id {
                let link = connection.core.create_object::<pw::link::Link>("link-factory", &properties! {
                    "link.output.node" => input.target.node.to_string(), "link.output.port" => input.target.id.to_string(),
                    "link.input.node" => node.to_string(), "link.input.port" => id.to_string(),
                    "link.passive" => "true", "object.linger" => "false",
                }).map_err(|e| CaptureError::Worker(format!("Could not monitor an application audio channel: {e}")))?;
                let ready = Rc::new(Cell::new(false));
                let link_ready = ready.clone();
                let failure = self.failure.clone();
                let listener = link
                    .add_listener_local()
                    .info(move |info| match info.state() {
                        pw::link::LinkState::Paused | pw::link::LinkState::Active => {
                            link_ready.set(true)
                        }
                        pw::link::LinkState::Error(message) => {
                            *failure.borrow_mut() =
                                Some(format!("Application audio link failed: {message}"))
                        }
                        _ => link_ready.set(false),
                    })
                    .register();
                input.link = Some(CaptureLink {
                    _listener: listener,
                    _link: link,
                    ready,
                });
            }
        }
        Ok(())
    }

    pub fn ready(&self) -> bool {
        self.data.connected.get()
            && self
                .data
                .inputs
                .borrow()
                .values()
                .all(|input| input.link.as_ref().is_some_and(|link| link.ready.get()))
    }
}

impl Drop for ApplicationMonitor {
    fn drop(&mut self) {
        for input in self.data.inputs.borrow_mut().values_mut() {
            drop(input.link.take());
        }
        // SAFETY: all fields referenced by callbacks are still alive, callbacks
        // are confined to this thread, and the filter is destroyed exactly once.
        unsafe {
            pw::sys::pw_filter_destroy(self.filter.as_ptr());
        }
    }
}
