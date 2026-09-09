use pipewire::{self as pw, properties::properties, spa};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// Several native playback clients from one process model an application's
/// independent audio streams. Every signal stays on the private test graph.
pub struct NativeTone {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl NativeTone {
    pub fn start(application: &str, name: &str, target: &str, frequency: f64) -> Self {
        let application = application.to_owned();
        let name = name.to_owned();
        let target = target.to_owned();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = thread::spawn(move || {
            pw::init();
            let main_loop = pw::main_loop::MainLoopRc::new(None).unwrap();
            let context = pw::context::ContextRc::new(&main_loop, Some(properties! {
                "application.id" => application.clone(), "application.name" => application.clone(),
            })).unwrap();
            let core = context.connect_rc(None).unwrap();
            let stream = pw::stream::StreamRc::new(core, "Prollyglot synthetic test signal", properties! {
                "application.id" => application.clone(), "application.name" => application,
                "node.name" => name, "target.object" => target,
                "media.type" => "Audio", "media.category" => "Playback",
                "node.dont-move" => "true", "node.dont-fallback" => "true", "node.dont-reconnect" => "true",
            }).unwrap();
            let _listener = stream
                .add_local_listener_with_user_data(0_u64)
                .process(move |stream, phase| {
                    let Some(mut buffer) = stream.dequeue_buffer() else {
                        return;
                    };
                    let requested = buffer.requested() as usize;
                    let Some(data) = buffer.datas_mut().first_mut() else {
                        return;
                    };
                    let Some(bytes) = data.data() else { return };
                    let frames =
                        (if requested == 0 { 1024 } else { requested }).min(bytes.len() / 4);
                    for sample in bytes[..frames * 4].chunks_exact_mut(4) {
                        let value = (std::f64::consts::TAU * frequency * *phase as f64 / 48000.0)
                            .sin() as f32
                            * 0.2;
                        sample.copy_from_slice(&value.to_ne_bytes());
                        *phase += 1;
                    }
                    let chunk = data.chunk_mut();
                    *chunk.offset_mut() = 0;
                    *chunk.size_mut() = (frames * 4) as u32;
                    *chunk.stride_mut() = 4;
                })
                .register()
                .unwrap();
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
            .unwrap()
            .0
            .into_inner();
            let pod = spa::pod::Pod::from_bytes(&values).unwrap();
            stream
                .connect(
                    spa::utils::Direction::Output,
                    None,
                    pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
                    &mut [pod],
                )
                .unwrap();
            while !worker_stop.load(Ordering::Acquire) {
                main_loop
                    .loop_()
                    .iterate(pw::loop_::Timeout::Finite(Duration::from_millis(20)));
            }
        });
        Self {
            stop,
            worker: Some(worker),
        }
    }
}

impl Drop for NativeTone {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
