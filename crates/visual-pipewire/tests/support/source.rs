use pipewire::{self as pw, properties::properties, spa};
use spa::{
    pod::{Object, Property, PropertyFlags, Value},
    utils::{Fraction, Id, Rectangle},
};
use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

fn property(key: u32, value: Value) -> Property {
    Property {
        key,
        flags: PropertyFlags::empty(),
        value,
    }
}
fn pod(type_: u32, id: u32, properties: Vec<Property>) -> Vec<u8> {
    spa::pod::serialize::PodSerializer::serialize(
        Cursor::new(Vec::new()),
        &Value::Object(Object {
            type_,
            id,
            properties,
        }),
    )
    .unwrap()
    .0
    .into_inner()
}
fn format(width: u32, height: u32, frame_rate: u32) -> Vec<u8> {
    use spa::sys::*;
    pod(
        SPA_TYPE_OBJECT_Format,
        SPA_PARAM_EnumFormat,
        vec![
            property(SPA_FORMAT_mediaType, Value::Id(Id(SPA_MEDIA_TYPE_video))),
            property(
                SPA_FORMAT_mediaSubtype,
                Value::Id(Id(SPA_MEDIA_SUBTYPE_raw)),
            ),
            property(
                SPA_FORMAT_VIDEO_format,
                Value::Id(Id(SPA_VIDEO_FORMAT_BGRx)),
            ),
            property(
                SPA_FORMAT_VIDEO_size,
                Value::Rectangle(Rectangle { width, height }),
            ),
            property(
                SPA_FORMAT_VIDEO_framerate,
                Value::Fraction(Fraction {
                    num: frame_rate,
                    denom: 1,
                }),
            ),
            property(
                SPA_FORMAT_VIDEO_maxFramerate,
                Value::Fraction(Fraction { num: 15, denom: 1 }),
            ),
        ],
    )
}

pub struct VideoSource {
    pub node: u32,
    pub serial: u64,
    pub frames_sent: Arc<std::sync::atomic::AtomicU64>,
    stop: Arc<AtomicBool>,
    resize: crossbeam_channel::Sender<(u32, u32)>,
    worker: Option<JoinHandle<()>>,
}

struct Data {
    width: u32,
    height: u32,
    marker: u8,
    image: Option<Vec<u8>>,
    frames: Arc<std::sync::atomic::AtomicU64>,
}

impl VideoSource {
    pub fn start(
        name: &str,
        marker: u8,
        width: u32,
        height: u32,
        image: Option<Vec<u8>>,
        frame_rate: u32,
    ) -> Self {
        super::private_session();
        let name = name.to_owned();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let frames_sent = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let frames = frames_sent.clone();
        let (ready_tx, ready_rx) = crossbeam_channel::bounded(1);
        let (resize, resize_rx) = crossbeam_channel::bounded(1);
        let worker = thread::spawn(move || {
            pw::init();
            let main_loop = pw::main_loop::MainLoopRc::new(None).unwrap();
            let context = pw::context::ContextRc::new(&main_loop, None).unwrap();
            let core = context.connect_rc(None).unwrap();
            let registry = core.get_registry_rc().unwrap();
            let search_name = name.clone();
            let _registry = registry
                .add_listener_local()
                .global(move |global| {
                    let Some(props) = global.props else { return };
                    if global.type_ == pw::types::ObjectType::Node
                        && props.get("node.name") == Some(search_name.as_str())
                    {
                        let serial = props.get("object.serial").unwrap().parse::<u64>().unwrap();
                        let _ = ready_tx.try_send((global.id, serial));
                    }
                })
                .register();
            let stream = pw::stream::StreamRc::new(core, "Prollyglot synthetic screen", properties! {
                "node.name" => name, "media.class" => "Video/Source", "media.type" => "Video",
                "media.category" => "Capture", "media.role" => "Screen", "node.driver" => "true",
            }).unwrap();
            let failure = std::rc::Rc::new(std::cell::RefCell::new(None));
            let stream_failure = failure.clone();
            let _listener = stream
                .add_local_listener_with_user_data(Data {
                    width,
                    height,
                    marker,
                    image,
                    frames,
                })
                .state_changed(move |_, _, _, state| {
                    if let pw::stream::StreamState::Error(message) = state {
                        *stream_failure.borrow_mut() = Some(message);
                    }
                })
                .param_changed(|stream, data, id, param| {
                    if id != spa::sys::SPA_PARAM_Format {
                        return;
                    }
                    let Some(param) = param else { return };
                    let mut format = spa::param::video::VideoInfoRaw::new();
                    format.parse(param).unwrap();
                    data.width = format.size().width;
                    data.height = format.size().height;
                    use spa::sys::*;
                    let bytes = pod(
                        SPA_TYPE_OBJECT_ParamBuffers,
                        SPA_PARAM_Buffers,
                        vec![
                            property(SPA_PARAM_BUFFERS_buffers, Value::Int(4)),
                            property(SPA_PARAM_BUFFERS_blocks, Value::Int(1)),
                            property(
                                SPA_PARAM_BUFFERS_size,
                                Value::Int((data.width * data.height * 4) as i32),
                            ),
                            property(
                                SPA_PARAM_BUFFERS_stride,
                                Value::Int((data.width * 4) as i32),
                            ),
                        ],
                    );
                    stream
                        .update_params(&mut [spa::pod::Pod::from_bytes(&bytes).unwrap()])
                        .unwrap();
                })
                .process(|stream, data| {
                    let Some(mut buffer) = stream.dequeue_buffer() else {
                        return;
                    };
                    let Some(mapped) = buffer.datas_mut().first_mut() else {
                        return;
                    };
                    let Some(bytes) = mapped.data() else { return };
                    let size = data.width as usize * data.height as usize * 4;
                    assert!(bytes.len() >= size);
                    let sequence = data.frames.fetch_add(1, Ordering::AcqRel) + 1;
                    if let Some(image) = &data.image {
                        bytes[..size].copy_from_slice(image);
                    } else {
                        for pixel in bytes[..size].chunks_exact_mut(4) {
                            pixel.copy_from_slice(&[data.marker, sequence as u8, 12, 0]);
                        }
                    }
                    let chunk = mapped.chunk_mut();
                    *chunk.offset_mut() = 0;
                    *chunk.stride_mut() = (data.width * 4) as i32;
                    *chunk.size_mut() = size as u32;
                })
                .register()
                .unwrap();
            let bytes = format(width, height, frame_rate);
            stream
                .connect(
                    spa::utils::Direction::Output,
                    None,
                    pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::DRIVER,
                    &mut [spa::pod::Pod::from_bytes(&bytes).unwrap()],
                )
                .unwrap();
            let mut last = Instant::now();
            while !worker_stop.load(Ordering::Acquire) {
                main_loop
                    .loop_()
                    .iterate(pw::loop_::Timeout::Finite(Duration::from_millis(5)));
                if let Some(message) = failure.borrow_mut().take() {
                    panic!("synthetic video: {message}");
                }
                if let Ok((w, h)) = resize_rx.try_recv() {
                    let bytes = format(w, h, frame_rate);
                    stream
                        .update_params(&mut [spa::pod::Pod::from_bytes(&bytes).unwrap()])
                        .unwrap();
                }
                if last.elapsed() >= Duration::from_millis(70) {
                    let _ = stream.trigger_process();
                    last = Instant::now();
                }
            }
        });
        let (node, serial) = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        Self {
            node,
            serial,
            frames_sent,
            stop,
            resize,
            worker: Some(worker),
        }
    }

    pub fn resize(&self, width: u32, height: u32) {
        self.resize.send((width, height)).unwrap();
    }
}

impl Drop for VideoSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
