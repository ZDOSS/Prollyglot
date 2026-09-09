use std::{
    cell::{Cell, RefCell},
    io::Cursor,
    os::fd::OwnedFd,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crossbeam_channel::Sender;
use pipewire::{self as pw, properties::properties, spa};
use prollyglot_visual_pipeline::{
    DEFAULT_LIVE_CAPTURE_FPS, LatestFrameSender, PixelFormat, PixelRect, VisualFrame,
};
use spa::{
    buffer::meta::{MetaHeader, MetaHeaderFlags, MetaVideoCrop, MetaVideoTransform},
    param::video::{VideoFormat, VideoInfoRaw},
    pod::{ChoiceValue, Object, Pod, Property, PropertyFlags, Value},
    utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Id, Rectangle},
};

use crate::{
    CaptureError, CaptureEvent, FrameGeometry,
    portal::{Target, failed},
};

const MAX_DIMENSION: u32 = 16_384;
const MAX_PIXELS: u64 = 33_554_432; // permits 8K, bounds a BGRA allocation at 128 MiB
const POLL: Duration = Duration::from_millis(20);
const START_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy)]
struct Format {
    width: u32,
    height: u32,
    rgba: bool,
}

impl Format {
    fn new(width: u32, height: u32, format: VideoFormat) -> Result<Self, CaptureError> {
        if width == 0
            || height == 0
            || width > MAX_DIMENSION
            || height > MAX_DIMENSION
            || u64::from(width) * u64::from(height) > MAX_PIXELS
        {
            return Err(failed(
                "The screen frame exceeds supported dimensions (up to 8K).",
            ));
        }
        let rgba = match format {
            VideoFormat::BGRA | VideoFormat::BGRx => false,
            VideoFormat::RGBA | VideoFormat::RGBx => true,
            _ => {
                return Err(failed(
                    "The desktop did not negotiate packed RGB screen pixels.",
                ));
            }
        };
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
}

#[derive(Clone, Copy)]
struct Chunk {
    offset: usize,
    size: usize,
    stride: i32,
}

/// Copy only the valid crop, normalize channel order, and orient the image as
/// displayed. SPA chunks may wrap at maxsize; padding is never sent to OCR.
fn copy_frame(
    bytes: &[u8],
    chunk: Chunk,
    format: Format,
    geometry: FrameGeometry,
    sequence: u64,
    captured_at_micros: u64,
) -> Result<VisualFrame, CaptureError> {
    if bytes.is_empty() || chunk.stride <= 0 {
        return Err(failed(
            "The screen buffer has no readable rows or uses an unsupported negative stride.",
        ));
    }
    let stride = chunk.stride as usize;
    let row_bytes = format.width as usize * 4;
    let required = stride
        .checked_mul(format.height as usize - 1)
        .and_then(|n| n.checked_add(row_bytes))
        .ok_or_else(|| failed("The screen buffer length overflowed."))?;
    if stride < row_bytes || required > chunk.size.min(bytes.len()) {
        return Err(failed(
            "The screen buffer is shorter than its declared pixel rows.",
        ));
    }
    let offset = chunk.offset % bytes.len();
    let crop = geometry.crop;
    let mut pixels = vec![0; geometry.width as usize * geometry.height as usize * 4];
    if geometry.transform == 0 {
        let row_bytes = crop.width as usize * 4;
        for y in 0..crop.height as usize {
            let start =
                (offset + (y + crop.y as usize) * stride + crop.x as usize * 4) % bytes.len();
            let first = row_bytes.min(bytes.len() - start);
            let row = &mut pixels[y * row_bytes..(y + 1) * row_bytes];
            row[..first].copy_from_slice(&bytes[start..start + first]);
            row[first..].copy_from_slice(&bytes[..row_bytes - first]);
            for pixel in row.chunks_exact_mut(4) {
                if format.rgba {
                    pixel.swap(0, 2);
                }
                pixel[3] = 255;
            }
        }
        return VisualFrame::new(
            sequence,
            captured_at_micros,
            geometry.width,
            geometry.height,
            geometry.width as usize * 4,
            PixelFormat::Bgra8,
            pixels,
        )
        .map_err(failed);
    }
    for y in 0..crop.height {
        for x in 0..crop.width {
            let raw =
                (offset + (y + crop.y) as usize * stride + (x + crop.x) as usize * 4) % bytes.len();
            let flip_x = if geometry.transform >= 4 {
                crop.width - 1 - x
            } else {
                x
            };
            let (out_x, out_y) = match geometry.transform % 4 {
                0 => (flip_x, y),
                1 => (y, crop.width - 1 - flip_x),
                2 => (crop.width - 1 - flip_x, crop.height - 1 - y),
                _ => (crop.height - 1 - y, flip_x),
            };
            let output = (out_y as usize * geometry.width as usize + out_x as usize) * 4;
            let first = bytes[raw];
            let green = bytes[(raw + 1) % bytes.len()];
            let third = bytes[(raw + 2) % bytes.len()];
            let (blue, red) = if format.rgba {
                (third, first)
            } else {
                (first, third)
            };
            // xRGB's padding byte is undefined; OCR receives opaque BGRA in
            // every format, never transparency inferred from that byte.
            pixels[output..output + 4].copy_from_slice(&[blue, green, red, 255]);
        }
    }
    VisualFrame::new(
        sequence,
        captured_at_micros,
        geometry.width,
        geometry.height,
        geometry.width as usize * 4,
        PixelFormat::Bgra8,
        pixels,
    )
    .map_err(failed)
}

fn geometry(
    format: Format,
    crop: Option<PixelRect>,
    transform: u32,
) -> Result<FrameGeometry, CaptureError> {
    let crop = crop.unwrap_or_else(|| PixelRect::full(format.width, format.height));
    if crop.width == 0
        || crop.height == 0
        || !crop.fits_within(format.width, format.height)
        || transform > 7
    {
        return Err(failed(
            "The screen buffer has an invalid crop or transform.",
        ));
    }
    let rotated = transform % 2 == 1;
    Ok(FrameGeometry {
        raw_width: format.width,
        raw_height: format.height,
        crop,
        transform,
        width: if rotated { crop.height } else { crop.width },
        height: if rotated { crop.width } else { crop.height },
    })
}

fn property(key: u32, value: Value) -> Property {
    Property {
        key,
        flags: PropertyFlags::empty(),
        value,
    }
}

fn object(type_: u32, id: u32, properties: Vec<Property>) -> Result<Vec<u8>, CaptureError> {
    spa::pod::serialize::PodSerializer::serialize(
        Cursor::new(Vec::new()),
        &Value::Object(Object {
            type_,
            id,
            properties,
        }),
    )
    .map(|(cursor, _)| cursor.into_inner())
    .map_err(failed)
}

fn format_pod() -> Result<Vec<u8>, CaptureError> {
    use spa::sys::*;
    object(
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
                Value::Choice(ChoiceValue::Id(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Enum {
                        default: Id(SPA_VIDEO_FORMAT_BGRx),
                        alternatives: vec![
                            Id(SPA_VIDEO_FORMAT_BGRx),
                            Id(SPA_VIDEO_FORMAT_BGRA),
                            Id(SPA_VIDEO_FORMAT_RGBx),
                            Id(SPA_VIDEO_FORMAT_RGBA),
                        ],
                    },
                ))),
            ),
            property(
                SPA_FORMAT_VIDEO_size,
                Value::Choice(ChoiceValue::Rectangle(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Range {
                        default: Rectangle {
                            width: 1920,
                            height: 1080,
                        },
                        min: Rectangle {
                            width: 1,
                            height: 1,
                        },
                        max: Rectangle {
                            width: MAX_DIMENSION,
                            height: MAX_DIMENSION,
                        },
                    },
                ))),
            ),
            // Variable-rate screen sources can emit only changed frames. Do not
            // require a fixed camera-style rate or manufacture duplicate images.
            property(
                SPA_FORMAT_VIDEO_framerate,
                Value::Choice(ChoiceValue::Fraction(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Range {
                        default: Fraction { num: 0, denom: 1 },
                        min: Fraction { num: 0, denom: 1 },
                        max: Fraction { num: 240, denom: 1 },
                    },
                ))),
            ),
            property(
                SPA_FORMAT_VIDEO_maxFramerate,
                Value::Choice(ChoiceValue::Fraction(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Range {
                        default: Fraction {
                            num: DEFAULT_LIVE_CAPTURE_FPS,
                            denom: 1,
                        },
                        min: Fraction { num: 1, denom: 1 },
                        max: Fraction { num: 240, denom: 1 },
                    },
                ))),
            ),
        ],
    )
}

fn buffer_params(stream: &pw::stream::Stream) -> Result<(), CaptureError> {
    use spa::sys::*;
    let mut pods = vec![object(
        SPA_TYPE_OBJECT_ParamBuffers,
        SPA_PARAM_Buffers,
        vec![property(
            SPA_PARAM_BUFFERS_dataType,
            Value::Choice(ChoiceValue::Int(Choice(
                ChoiceFlags::empty(),
                ChoiceEnum::Flags {
                    default: (1 << SPA_DATA_MemPtr) | (1 << SPA_DATA_MemFd),
                    flags: vec![],
                },
            ))),
        )],
    )?];
    for (kind, size) in [
        (SPA_META_Header, std::mem::size_of::<spa_meta_header>()),
        (SPA_META_VideoCrop, std::mem::size_of::<spa_meta_region>()),
        (
            SPA_META_VideoTransform,
            std::mem::size_of::<spa_meta_videotransform>(),
        ),
    ] {
        pods.push(object(
            SPA_TYPE_OBJECT_ParamMeta,
            SPA_PARAM_Meta,
            vec![
                property(SPA_PARAM_META_type, Value::Id(Id(kind))),
                property(SPA_PARAM_META_size, Value::Int(size as i32)),
            ],
        )?);
    }
    let mut refs = pods
        .iter()
        .map(|bytes| Pod::from_bytes(bytes).ok_or_else(|| failed("Invalid buffer parameter.")))
        .collect::<Result<Vec<_>, _>>()?;
    stream.update_params(&mut refs).map_err(failed)
}

struct StreamData {
    format: Option<Format>,
    last_geometry: Option<FrameGeometry>,
    frames: LatestFrameSender,
    events: Sender<CaptureEvent>,
    failure: Rc<RefCell<Option<CaptureError>>>,
    delivered: Rc<Cell<bool>>,
    sequence: u64,
    started: Instant,
}

impl StreamData {
    fn process(&mut self, stream: &pw::stream::Stream) -> Result<(), CaptureError> {
        let Some(format) = self.format else {
            return Ok(());
        };
        let Some(mut buffer) = stream.dequeue_buffer() else {
            return Ok(());
        };
        // Bound work even if the producer outruns us. Returning old buffers
        // immediately prevents queued capture latency before the shared queue.
        for _ in 0..8 {
            let Some(newest) = stream.dequeue_buffer() else {
                break;
            };
            buffer = newest;
        }
        if buffer.find_meta::<MetaHeader>().is_some_and(|header| {
            header
                .flags()
                .intersects(MetaHeaderFlags::CORRUPTED | MetaHeaderFlags::GAP)
        }) {
            return Ok(());
        }
        let crop = buffer
            .find_meta::<MetaVideoCrop>()
            .map(|crop| crop.meta_region())
            .filter(|crop| crop.is_valid())
            .map(|crop| {
                let position = crop.position();
                if position.x < 0 || position.y < 0 {
                    return Err(failed("The screen crop has a negative origin."));
                }
                Ok(PixelRect {
                    x: position.x as u32,
                    y: position.y as u32,
                    width: crop.size().width,
                    height: crop.size().height,
                })
            })
            .transpose()?;
        let transform = buffer
            .find_meta::<MetaVideoTransform>()
            .map_or(0, |meta| meta.transform().as_raw());
        let geometry = geometry(format, crop, transform)?;
        let data = buffer.datas_mut();
        if data.len() != 1 {
            return Err(failed("The screen buffer is not a packed RGB plane."));
        }
        let mapped = &mut data[0];
        let chunk = mapped.chunk();
        if chunk.as_raw().flags & spa::sys::SPA_CHUNK_FLAG_CORRUPTED as i32 != 0 {
            return Ok(());
        }
        let empty = chunk.as_raw().flags & spa::sys::SPA_CHUNK_FLAG_EMPTY as i32 != 0;
        if chunk.size() == 0 && !empty {
            return Ok(());
        }
        let chunk = Chunk {
            offset: chunk.offset() as usize,
            size: chunk.size() as usize,
            stride: chunk.stride(),
        };
        if !empty
            && !matches!(
                mapped.type_(),
                spa::buffer::DataType::MemPtr | spa::buffer::DataType::MemFd
            )
        {
            return Err(failed(
                "The desktop supplied GPU-only screen buffers; CPU-readable capture is required.",
            ));
        }
        self.sequence = self.sequence.saturating_add(1);
        let captured = self.started.elapsed().as_micros() as u64;
        let frame = if empty {
            // EMPTY declares neutral (black) video, not meaningful mapped
            // bytes. Publish black to clear old OCR evidence without reading it.
            let mut pixels = vec![0; geometry.width as usize * geometry.height as usize * 4];
            for pixel in pixels.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
            VisualFrame::new(
                self.sequence,
                captured,
                geometry.width,
                geometry.height,
                geometry.width as usize * 4,
                PixelFormat::Bgra8,
                pixels,
            )
            .map_err(failed)?
        } else {
            let bytes = mapped
                .data()
                .ok_or_else(|| failed("The desktop screen buffer could not be mapped."))?;
            copy_frame(bytes, chunk, format, geometry, self.sequence, captured)?
        };
        if self.last_geometry != Some(geometry) {
            self.events
                .try_send(CaptureEvent::FormatChanged(geometry))
                .map_err(|_| failed("The screen event consumer is not keeping up."))?;
            self.last_geometry = Some(geometry);
        }
        let _ = self.frames.send(frame);
        self.delivered.set(true);
        Ok(())
    }
}

pub(crate) fn run(
    fd: OwnedFd,
    target: Target,
    stop: Arc<AtomicBool>,
    events: Sender<CaptureEvent>,
    frames: LatestFrameSender,
) -> Result<(), CaptureError> {
    pw::init();
    let main_loop = pw::main_loop::MainLoopRc::new(None).map_err(failed)?;
    let context = pw::context::ContextRc::new(
        &main_loop,
        Some(properties! {
            "application.name" => "Prollyglot", "application.id" => "com.prollyglot.desktop",
        }),
    )
    .map_err(failed)?;
    // Never connect to the unrestricted default PipeWire remote for capture.
    let core = context.connect_fd_rc(fd, None).map_err(failed)?;
    let failure = Rc::new(RefCell::new(None));
    let core_failure = failure.clone();
    let _core_listener = core
        .add_listener_local()
        .error(move |id, _, _, message| {
            if id == pw::core::PW_ID_CORE {
                *core_failure.borrow_mut() = Some(failed(format!(
                    "The screen PipeWire connection ended: {message}"
                )));
            }
        })
        .register();
    let registry = core.get_registry_rc().map_err(failed)?;
    let selected_id = Rc::new(Cell::new(None));
    let found = selected_id.clone();
    let removed = selected_id.clone();
    let removed_failure = failure.clone();
    let registry_target = target.clone();
    let _registry_listener = registry
        .add_listener_local()
        .global(move |global| {
            if global.type_ != pw::types::ObjectType::Node {
                return;
            }
            let matches = if let Some(serial) = registry_target.serial {
                global
                    .props
                    .and_then(|props| props.get("object.serial"))
                    .and_then(|value| value.parse::<u64>().ok())
                    == Some(serial)
            } else {
                global.id == registry_target.node_id
            };
            if matches {
                found.set(Some(global.id));
            }
        })
        .global_remove(move |id| {
            if removed.get() == Some(id) {
                *removed_failure.borrow_mut() = Some(CaptureError::Closed);
            }
        })
        .register();
    let mut props = properties! {
        "application.name" => "Prollyglot", "application.id" => "com.prollyglot.desktop",
        "media.type" => "Video", "media.category" => "Capture", "media.role" => "Screen",
        "node.name" => "prollyglot.screen-capture", "node.dont-fallback" => "true",
        "node.dont-reconnect" => "true", "node.dont-move" => "true",
    };
    if let Some(serial) = target.serial {
        props.insert("target.object", serial.to_string());
    }
    let stream =
        pw::stream::StreamRc::new(core, "Prollyglot selected screen", props).map_err(failed)?;
    let delivered = Rc::new(Cell::new(false));
    let started = Instant::now();
    let _listener = stream
        .add_local_listener_with_user_data(StreamData {
            format: None,
            last_geometry: None,
            frames,
            events,
            failure: failure.clone(),
            delivered: delivered.clone(),
            sequence: 0,
            started,
        })
        .state_changed(|_, data, old, state| match state {
            pw::stream::StreamState::Error(message) => {
                *data.failure.borrow_mut() = Some(failed(message))
            }
            pw::stream::StreamState::Unconnected if old != pw::stream::StreamState::Unconnected => {
                *data.failure.borrow_mut() = Some(CaptureError::Closed)
            }
            _ => {}
        })
        .param_changed(|stream, data, id, param| {
            if id != spa::sys::SPA_PARAM_Format {
                return;
            }
            data.format = None;
            let Some(param) = param else { return };
            let result = (|| {
                let mut raw = VideoInfoRaw::new();
                raw.parse(param).map_err(failed)?;
                if raw
                    .flags()
                    .contains(spa::param::video::VideoFlags::MODIFIER)
                    || raw.interlace_mode() != spa::param::video::VideoInterlaceMode::Progressive
                    || raw.views() > 1
                {
                    return Err(failed(
                        "Screen capture requires progressive, linear CPU-readable pixels.",
                    ));
                }
                let format = Format::new(raw.size().width, raw.size().height, raw.format())?;
                buffer_params(stream)?;
                data.format = Some(format);
                Ok(())
            })();
            if let Err(error) = result {
                *data.failure.borrow_mut() = Some(error);
            }
        })
        .process(|stream, data| {
            if let Err(error) = data.process(stream) {
                *data.failure.borrow_mut() = Some(error);
            }
        })
        .register()
        .map_err(failed)?;
    let bytes = format_pod()?;
    let pod = Pod::from_bytes(&bytes).ok_or_else(|| failed("Invalid screen format."))?;
    // Pre-v6 portals supply only node ID. Never reconnect or fall back after
    // destruction: a newly allocated node could belong to a different source.
    stream
        .connect(
            spa::utils::Direction::Input,
            if target.serial.is_some() {
                None
            } else {
                Some(target.node_id)
            },
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut [pod],
        )
        .map_err(failed)?;
    // No RT_PROCESS: copying, allocation and publication stay off the graph's
    // real-time thread. All native listeners drop before their stream/core.
    while !stop.load(Ordering::Acquire) {
        main_loop.loop_().iterate(pw::loop_::Timeout::Finite(POLL));
        if let Some(error) = failure.borrow_mut().take() {
            return Err(error);
        }
        if !delivered.get() && started.elapsed() >= START_TIMEOUT {
            return Err(failed(
                "The selected screen source did not provide a readable frame.",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_all_channel_orders_without_using_x_or_alpha_as_opacity() {
        for (input, expected) in [
            (VideoFormat::BGRA, [10, 20, 30, 255]),
            (VideoFormat::BGRx, [10, 20, 30, 255]),
            (VideoFormat::RGBA, [30, 20, 10, 255]),
            (VideoFormat::RGBx, [30, 20, 10, 255]),
        ] {
            let format = Format::new(1, 1, input).unwrap();
            let frame = copy_frame(
                &[10, 20, 30, 0],
                Chunk {
                    offset: 0,
                    size: 4,
                    stride: 4,
                },
                format,
                geometry(format, None, 0).unwrap(),
                1,
                99,
            )
            .unwrap();
            assert_eq!(frame.pixels(), expected);
            assert_eq!(frame.captured_at_micros, 99);
        }
    }

    #[test]
    fn removes_padding_and_crops_wrapped_chunks_before_publication() {
        let format = Format::new(3, 2, VideoFormat::BGRA).unwrap();
        let mut bytes = [237u8; 32];
        for (index, color) in [10u8, 20, 30, 40, 50, 60].into_iter().enumerate() {
            for (channel, value) in [color, 0, 0, 0].into_iter().enumerate() {
                bytes[(7 + index / 3 * 16 + index % 3 * 4 + channel) % 32] = value;
            }
        }
        let crop = PixelRect {
            x: 1,
            y: 0,
            width: 2,
            height: 2,
        };
        let frame = copy_frame(
            &bytes,
            Chunk {
                offset: 39,
                size: 100,
                stride: 16,
            },
            format,
            geometry(format, Some(crop), 0).unwrap(),
            1,
            0,
        )
        .unwrap();
        assert_eq!(
            frame.pixels(),
            [20, 0, 0, 255, 30, 0, 0, 255, 50, 0, 0, 255, 60, 0, 0, 255]
        );
        assert_eq!(frame.stride, 8);
    }

    #[test]
    fn applies_every_spa_transform_in_display_order() {
        let format = Format::new(3, 2, VideoFormat::BGRA).unwrap();
        let bytes = (1u8..=6).flat_map(|x| [x, 0, 0, 255]).collect::<Vec<_>>();
        for (transform, expected) in [
            [1, 2, 3, 4, 5, 6],
            [3, 6, 2, 5, 1, 4],
            [6, 5, 4, 3, 2, 1],
            [4, 1, 5, 2, 6, 3],
            [3, 2, 1, 6, 5, 4],
            [1, 4, 2, 5, 3, 6],
            [4, 5, 6, 1, 2, 3],
            [6, 3, 5, 2, 4, 1],
        ]
        .into_iter()
        .enumerate()
        {
            let frame = copy_frame(
                &bytes,
                Chunk {
                    offset: 0,
                    size: 24,
                    stride: 12,
                },
                format,
                geometry(format, None, transform as u32).unwrap(),
                1,
                0,
            )
            .unwrap();
            assert_eq!(
                frame
                    .pixels()
                    .chunks_exact(4)
                    .map(|p| p[0])
                    .collect::<Vec<_>>(),
                expected,
                "transform {transform}"
            );
            assert_eq!(
                (frame.width, frame.height),
                if transform % 2 == 0 { (3, 2) } else { (2, 3) }
            );
        }
    }

    #[test]
    fn rejects_unsupported_formats_and_unsafe_sizes_before_allocation() {
        for (w, h, f) in [
            (0, 20, VideoFormat::BGRA),
            (20, 0, VideoFormat::BGRA),
            (u32::MAX, 1, VideoFormat::BGRA),
            (8192, 8192, VideoFormat::BGRA),
            (2, 2, VideoFormat::NV12),
        ] {
            assert!(Format::new(w, h, f).is_err());
        }
        assert!(Format::new(7680, 4320, VideoFormat::BGRx).is_ok());
        let format = Format::new(3, 2, VideoFormat::BGRA).unwrap();
        assert!(
            geometry(
                format,
                Some(PixelRect {
                    x: 2,
                    y: 1,
                    width: 2,
                    height: 2
                }),
                0
            )
            .is_err()
        );
        assert!(geometry(format, None, 8).is_err());
        for chunk in [
            Chunk {
                offset: 0,
                size: 24,
                stride: 8,
            },
            Chunk {
                offset: 0,
                size: 23,
                stride: 12,
            },
            Chunk {
                offset: 0,
                size: 24,
                stride: -12,
            },
            Chunk {
                offset: 0,
                size: 24,
                stride: i32::MAX,
            },
        ] {
            assert!(
                copy_frame(
                    &[0; 24],
                    chunk,
                    format,
                    geometry(format, None, 0).unwrap(),
                    1,
                    0
                )
                .is_err()
            );
        }
    }
}
