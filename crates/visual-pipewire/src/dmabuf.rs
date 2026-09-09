//! Import only negotiated DMA-BUFs through EGL. No display server connection,
//! screen/window surface, raw-pixel IPC, or producer FD ownership is involved.
//! This module and every graphics object stay on the capture worker thread.

use crate::{CaptureError, portal::failed};
use glow::HasContext;
use khronos_egl as egl;
use pipewire::spa::{
    buffer::{Data, DataType},
    param::video::VideoFormat,
};
use std::{
    ffi::c_void,
    ptr,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(crate) const IMPLICIT_MODIFIER: u64 = (1 << 56) - 1; // DRM_FORMAT_MOD_INVALID
#[cfg(test)]
#[path = "dmabuf_hardware.rs"]
mod hardware_tests;
const MAX_DEVICES: usize = 8;
const MAX_MODIFIERS: usize = 256;
const READBACK_TIMEOUT: Duration = Duration::from_millis(250);
type Egl = egl::DynamicInstance<egl::EGL1_5>;
type QueryDevices = unsafe extern "system" fn(i32, *mut *mut c_void, *mut i32) -> u32;
type QueryFormats = unsafe extern "system" fn(*mut c_void, i32, *mut i32, *mut i32) -> u32;
type QueryModifiers =
    unsafe extern "system" fn(*mut c_void, i32, i32, *mut u64, *mut u32, *mut i32) -> u32;
type CreateImage = unsafe extern "system" fn(
    *mut c_void,
    *mut c_void,
    u32,
    *mut c_void,
    *const i32,
) -> *mut c_void;
type DestroyImage = unsafe extern "system" fn(*mut c_void, *mut c_void) -> u32;
type BindImage = unsafe extern "system" fn(u32, *mut c_void);
type UnmapBuffer = unsafe extern "system" fn(u32) -> u8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Format {
    pub video: VideoFormat,
    pub modifier: u64,
}

fn fourcc(video: VideoFormat) -> Result<i32, CaptureError> {
    let bytes = match video {
        VideoFormat::BGRx => *b"XR24",
        VideoFormat::BGRA => *b"AR24",
        VideoFormat::RGBx => *b"XB24",
        VideoFormat::RGBA => *b"AB24",
        _ => return Err(failed("GPU capture requires packed 8-bit RGB pixels.")),
    };
    Ok(i32::from_le_bytes(bytes))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plane {
    pub fd: i32,
    pub offset: i32,
    pub stride: i32,
}

pub(crate) fn planes(data: &[Data]) -> Result<Vec<Plane>, CaptureError> {
    if data.is_empty() || data.len() > 4 {
        return Err(failed(
            "The GPU screen buffer must have one to four image planes.",
        ));
    }
    data.iter()
        .map(|plane| {
            if plane.type_() != DataType::DmaBuf || plane.as_raw().chunk.is_null() {
                return Err(failed(
                    "The GPU screen buffer contains an invalid image plane.",
                ));
            }
            let fd = i32::try_from(plane.as_raw().fd)
                .ok()
                .filter(|fd| *fd >= 0)
                .ok_or_else(|| failed("The GPU screen buffer has an invalid file descriptor."))?;
            let offset = plane
                .as_raw()
                .mapoffset
                .checked_add(plane.chunk().offset())
                .and_then(|offset| i32::try_from(offset).ok())
                .ok_or_else(|| failed("The GPU screen buffer offset is too large."))?;
            let stride = plane.chunk().stride();
            if stride <= 0 {
                return Err(failed(
                    "GPU screen capture requires a positive plane stride.",
                ));
            }
            // DMA-BUF maxsize/size can be zero and tiled planes need not describe
            // linear pixel rows. EGL/the kernel validate the actual allocation.
            Ok(Plane { fd, offset, stride })
        })
        .collect()
}

fn attributes(
    format: Format,
    width: u32,
    height: u32,
    planes: &[Plane],
) -> Result<Vec<i32>, CaptureError> {
    if width == 0
        || height == 0
        || width > 16_384
        || height > 16_384
        || u64::from(width) * u64::from(height) > 33_554_432
        || planes.is_empty()
        || planes.len() > 4
    {
        return Err(failed(
            "The GPU screen image exceeds supported dimensions or plane counts.",
        ));
    }
    let mut attrs = vec![
        0x30D2, // EGL_IMAGE_PRESERVED_KHR: importing must preserve producer pixels.
        1,
        egl::WIDTH,
        width as i32,
        egl::HEIGHT,
        height as i32,
        0x3271,
        fourcc(format.video)?,
    ];
    let keys = [
        (0x3272, 0x3273, 0x3274, 0x3443, 0x3444),
        (0x3275, 0x3276, 0x3277, 0x3445, 0x3446),
        (0x3278, 0x3279, 0x327A, 0x3447, 0x3448),
        (0x3440, 0x3441, 0x3442, 0x3449, 0x344A),
    ];
    for (plane, (fd, offset, pitch, low, high)) in planes.iter().zip(keys) {
        if plane.fd < 0 || plane.offset < 0 || plane.stride <= 0 {
            return Err(failed("Invalid GPU image plane."));
        }
        attrs.extend([fd, plane.fd, offset, plane.offset, pitch, plane.stride]);
        if format.modifier != IMPLICIT_MODIFIER {
            attrs.extend([
                low,
                format.modifier as u32 as i32,
                high,
                (format.modifier >> 32) as u32 as i32,
            ]);
        }
    }
    attrs.push(egl::NONE);
    Ok(attrs)
}

// Each call site supplies the signature from EGL/GLES headers. The dynamic
// instance outlives these function pointers and all objects that use them.
unsafe fn proc<T: Copy>(egl: &Egl, name: &str) -> Result<T, CaptureError> {
    let pointer = egl
        .get_proc_address(name)
        .ok_or_else(|| failed(format!("Missing graphics function {name}.")))?;
    assert_eq!(std::mem::size_of::<T>(), std::mem::size_of_val(&pointer));
    Ok(unsafe { std::mem::transmute_copy(&pointer) })
}

struct Session {
    egl: Rc<Egl>,
    display: egl::Display,
    context: Option<egl::Context>,
    surface: Option<egl::Surface>,
}
impl Session {
    fn open(egl: Rc<Egl>, display: egl::Display) -> Result<Self, CaptureError> {
        egl.initialize(display).map_err(failed)?;
        Ok(Self {
            egl,
            display,
            context: None,
            surface: None,
        })
    }
    fn current(&self) -> Result<(), CaptureError> {
        self.egl
            .make_current(self.display, self.surface, self.surface, self.context)
            .map_err(failed)
    }
    fn context(&mut self) -> Result<(), CaptureError> {
        self.egl.bind_api(egl::OPENGL_ES_API).map_err(failed)?;
        let config = self
            .egl
            .choose_first_config(
                self.display,
                &[
                    egl::SURFACE_TYPE,
                    egl::PBUFFER_BIT,
                    egl::RENDERABLE_TYPE,
                    egl::OPENGL_ES3_BIT,
                    egl::RED_SIZE,
                    8,
                    egl::GREEN_SIZE,
                    8,
                    egl::BLUE_SIZE,
                    8,
                    egl::NONE,
                ],
            )
            .map_err(failed)?
            .ok_or_else(|| failed("No offscreen GLES 3 configuration."))?;
        self.context = Some(
            self.egl
                .create_context(
                    self.display,
                    config,
                    None,
                    &[egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE],
                )
                .map_err(failed)?,
        );
        // A private 1x1 pbuffer works without relying on surfaceless-context
        // extensions. It is not a native desktop window or display capture.
        self.surface = Some(
            self.egl
                .create_pbuffer_surface(
                    self.display,
                    config,
                    &[egl::WIDTH, 1, egl::HEIGHT, 1, egl::NONE],
                )
                .map_err(failed)?,
        );
        self.current()
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.egl.make_current(self.display, None, None, None);
        if let Some(context) = self.context.take() {
            let _ = self.egl.destroy_context(self.display, context);
        }
        if let Some(surface) = self.surface.take() {
            let _ = self.egl.destroy_surface(self.display, surface);
        }
        let _ = self.egl.terminate(self.display);
    }
}

struct Device {
    session: Session,
    gl: glow::Context,
    formats: Vec<Format>,
    create_image: CreateImage,
    destroy_image: DestroyImage,
    bind_image: BindImage,
    unmap_buffer: UnmapBuffer,
    max_size: u32,
}

impl Device {
    fn open(mut session: Session, require_dmabuf: bool) -> Result<Self, CaptureError> {
        let extensions = session
            .egl
            .query_string(Some(session.display), egl::EXTENSIONS)
            .map_err(failed)?
            .to_string_lossy();
        let has = |name| {
            extensions
                .split_ascii_whitespace()
                .any(|extension| extension == name)
        };
        if require_dmabuf && !has("EGL_EXT_image_dma_buf_import") {
            return Err(failed("This graphics device cannot import DMA-BUF images."));
        }
        let mut formats = Vec::new();
        let mut supported_formats = None;
        if require_dmabuf && has("EGL_EXT_image_dma_buf_import_modifiers") {
            let query: QueryFormats = unsafe { proc(&session.egl, "eglQueryDmaBufFormatsEXT")? };
            let mut values = [0i32; 256];
            let mut count = 0;
            if unsafe {
                query(
                    session.display.as_ptr(),
                    values.len() as i32,
                    values.as_mut_ptr(),
                    &mut count,
                )
            } == 0
                || !(0..=values.len() as i32).contains(&count)
            {
                return Err(failed("Could not query GPU image formats."));
            }
            supported_formats = Some(values[..count as usize].to_vec());
        }
        for video in [
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA,
        ] {
            if !require_dmabuf {
                break;
            }
            if supported_formats
                .as_ref()
                .is_some_and(|formats| !formats.contains(&fourcc(video).unwrap()))
            {
                continue;
            }
            if has("EGL_EXT_image_dma_buf_import_modifiers") {
                let query: QueryModifiers =
                    unsafe { proc(&session.egl, "eglQueryDmaBufModifiersEXT")? };
                let mut modifiers = [0u64; MAX_MODIFIERS];
                let mut external = [0u32; MAX_MODIFIERS];
                let mut count = 0;
                let success = unsafe {
                    query(
                        session.display.as_ptr(),
                        fourcc(video)?,
                        MAX_MODIFIERS as i32,
                        modifiers.as_mut_ptr(),
                        external.as_mut_ptr(),
                        &mut count,
                    )
                };
                if success != 0 && (0..=MAX_MODIFIERS as i32).contains(&count) {
                    for index in 0..count as usize {
                        // External-only textures cannot be attached to our
                        // read framebuffer. Never advertise an unreadable pair.
                        if external[index] == 0 {
                            formats.push(Format {
                                video,
                                modifier: modifiers[index],
                            });
                        }
                    }
                }
            } else {
                // Legacy exporters (including older drivers) can use implicit
                // modifiers. Import failure still falls back to shared memory.
                formats.push(Format {
                    video,
                    modifier: IMPLICIT_MODIFIER,
                });
            }
        }
        if require_dmabuf && formats.is_empty() {
            return Err(failed("No readable RGB DMA-BUF modifiers."));
        }
        session.context()?;
        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                session
                    .egl
                    .get_proc_address(name)
                    .map_or(ptr::null(), |f| f as *const c_void)
            })
        };
        if require_dmabuf && !gl.supported_extensions().contains("GL_OES_EGL_image") {
            return Err(failed("This GLES device cannot bind EGL images."));
        }
        let max_size = unsafe { gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE) }.max(0) as u32;
        Ok(Self {
            create_image: unsafe { proc(&session.egl, "eglCreateImageKHR")? },
            destroy_image: unsafe { proc(&session.egl, "eglDestroyImageKHR")? },
            bind_image: unsafe { proc(&session.egl, "glEGLImageTargetTexture2DOES")? },
            unmap_buffer: unsafe { proc(&session.egl, "glUnmapBuffer")? },
            session,
            gl,
            formats,
            max_size,
        })
    }

    fn read(
        &self,
        format: Format,
        width: u32,
        height: u32,
        planes: &[Plane],
        stop: &AtomicBool,
    ) -> Result<Vec<u8>, CaptureError> {
        let attrs = attributes(format, width, height, planes)?;
        if width > self.max_size || height > self.max_size {
            return Err(failed(
                "The GPU screen image exceeds the graphics device's texture limit.",
            ));
        }
        self.session.current()?;
        let mut objects = Objects::new(self);
        // SAFETY: checked sizes/attributes and borrowed producer FDs. EGL takes
        // its own reference, never ownership of the PipeWire descriptors.
        objects.image = unsafe {
            (self.create_image)(
                self.session.display.as_ptr(),
                ptr::null_mut(),
                0x3270,
                ptr::null_mut(),
                attrs.as_ptr(),
            )
        };
        if objects.image.is_null() {
            return Err(failed("EGL could not import this screen buffer."));
        }
        unsafe {
            objects.texture = Some(self.gl.create_texture().map_err(failed)?);
            self.gl.bind_texture(glow::TEXTURE_2D, objects.texture);
            (self.bind_image)(glow::TEXTURE_2D, objects.image);
            if self.gl.get_error() != glow::NO_ERROR {
                return Err(failed("GLES could not bind this screen buffer."));
            }
        }
        objects.read(width, height, stop)
    }
}

/// Per-frame native ownership also covers every early error/Stop path.
struct Objects<'a> {
    device: &'a Device,
    image: *mut c_void,
    texture: Option<glow::Texture>,
    framebuffer: Option<glow::Framebuffer>,
    buffer: Option<glow::Buffer>,
    fence: Option<glow::Fence>,
}
impl<'a> Objects<'a> {
    fn new(device: &'a Device) -> Self {
        Self {
            device,
            image: ptr::null_mut(),
            texture: None,
            framebuffer: None,
            buffer: None,
            fence: None,
        }
    }
    fn read(
        &mut self,
        width: u32,
        height: u32,
        stop: &AtomicBool,
    ) -> Result<Vec<u8>, CaptureError> {
        let gl = &self.device.gl;
        let length = width as usize * height as usize * 4;
        // SAFETY: the current context, texture and every created object belong
        // to this worker; dimensions were bounded before allocation/import.
        unsafe {
            self.framebuffer = Some(gl.create_framebuffer().map_err(failed)?);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, self.framebuffer);
            gl.framebuffer_texture_2d(
                glow::READ_FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                self.texture,
                0,
            );
            if gl.check_framebuffer_status(glow::READ_FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                return Err(failed("The imported screen image is not readable by GLES."));
            }
            self.buffer = Some(gl.create_buffer().map_err(failed)?);
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, self.buffer);
            gl.buffer_data_size(glow::PIXEL_PACK_BUFFER, length as i32, glow::STREAM_READ);
            gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
            if gl.get_error() != glow::NO_ERROR
                || gl.get_buffer_parameter_i32(glow::PIXEL_PACK_BUFFER, glow::BUFFER_SIZE)
                    != length as i32
            {
                return Err(failed("Could not allocate the screen readback buffer."));
            }
            gl.read_pixels(
                0,
                0,
                width as i32,
                height as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::BufferOffset(0),
            );
            if gl.get_error() != glow::NO_ERROR {
                return Err(failed("GPU screen readback failed."));
            }
            let fence = gl
                .fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0)
                .map_err(failed)?;
            self.fence = Some(fence);
            gl.flush();
            let started = Instant::now();
            loop {
                if stop.load(Ordering::Acquire) {
                    return Err(CaptureError::Cancelled);
                }
                match gl.client_wait_sync(fence, 0, 0) {
                    glow::ALREADY_SIGNALED | glow::CONDITION_SATISFIED => break,
                    glow::TIMEOUT_EXPIRED if started.elapsed() < READBACK_TIMEOUT => {
                        std::thread::sleep(Duration::from_millis(1))
                    }
                    _ => return Err(failed("GPU screen readback did not finish within 250 ms.")),
                }
            }
            let mapped = gl.map_buffer_range(
                glow::PIXEL_PACK_BUFFER,
                0,
                length as i32,
                glow::MAP_READ_BIT,
            );
            if mapped.is_null() {
                return Err(failed("GPU screen pixels could not be mapped."));
            }
            let pixels = std::slice::from_raw_parts(mapped, length).to_vec();
            if (self.device.unmap_buffer)(glow::PIXEL_PACK_BUFFER) == 0 {
                return Err(failed("GPU screen pixels became invalid during readback."));
            }
            Ok(pixels)
        }
    }
}
impl Drop for Objects<'_> {
    fn drop(&mut self) {
        // SAFETY: all handles originate in this current context. Deferred GL
        // deletion and implicit DMA-BUF synchronization retain in-flight uses.
        unsafe {
            let gl = &self.device.gl;
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
            gl.bind_texture(glow::TEXTURE_2D, None);
            if let Some(fence) = self.fence.take() {
                gl.delete_sync(fence);
            }
            if let Some(buffer) = self.buffer.take() {
                gl.delete_buffer(buffer);
            }
            if let Some(framebuffer) = self.framebuffer.take() {
                gl.delete_framebuffer(framebuffer);
            }
            if let Some(texture) = self.texture.take() {
                gl.delete_texture(texture);
            }
            if !self.image.is_null() {
                (self.device.destroy_image)(self.device.session.display.as_ptr(), self.image);
            }
        }
    }
}

pub(crate) struct Importer {
    devices: Vec<Device>,
}
impl Importer {
    pub fn open() -> Result<Self, CaptureError> {
        // Never call eglGetDisplay(EGL_DEFAULT_DISPLAY): that could connect to
        // the owner's X11/Wayland session. Device displays have no windowing API.
        let egl = Rc::new(unsafe { Egl::load_required() }.map_err(failed)?);
        let extensions = egl
            .query_string(None, egl::EXTENSIONS)
            .map_err(failed)?
            .to_string_lossy();
        if !extensions
            .split_ascii_whitespace()
            .any(|e| e == "EGL_EXT_platform_device")
        {
            return Err(failed("EGL device displays are unavailable."));
        }
        let query: QueryDevices = unsafe { proc(&egl, "eglQueryDevicesEXT")? };
        let mut devices = [ptr::null_mut(); MAX_DEVICES];
        let mut count = 0;
        if unsafe { query(MAX_DEVICES as i32, devices.as_mut_ptr(), &mut count) } == 0
            || !(0..=MAX_DEVICES as i32).contains(&count)
        {
            return Err(failed("Could not enumerate EGL capture devices."));
        }
        let devices = devices[..count as usize]
            .iter()
            .filter_map(|device| {
                if device.is_null() {
                    return None;
                }
                let display =
                    unsafe { egl.get_platform_display(0x313F, *device, &[egl::ATTRIB_NONE]) }
                        .ok()?;
                Device::open(Session::open(Rc::clone(&egl), display).ok()?, true).ok()
            })
            .collect::<Vec<_>>();
        if devices.is_empty() {
            return Err(failed(
                "No graphics device supports readable RGB DMA-BUF capture.",
            ));
        }
        Ok(Self { devices })
    }
    pub fn formats(&self) -> Vec<Format> {
        let mut result = Vec::new();
        for format in self.devices.iter().flat_map(|device| &device.formats) {
            if !result.contains(format) {
                result.push(*format);
            }
        }
        result
    }
    pub fn read(
        &self,
        format: Format,
        width: u32,
        height: u32,
        planes: &[Plane],
        stop: &AtomicBool,
    ) -> Result<Vec<u8>, CaptureError> {
        let mut error = failed("No GPU supports the selected screen format/modifier.");
        for device in self
            .devices
            .iter()
            .filter(|device| device.formats.contains(&format))
        {
            if stop.load(Ordering::Acquire) {
                return Err(CaptureError::Cancelled);
            }
            match device.read(format, width, height, planes, stop) {
                Ok(pixels) => return Ok(pixels),
                Err(CaptureError::Cancelled) => return Err(CaptureError::Cancelled),
                Err(next) => error = next,
            }
        }
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;

    #[test]
    fn rgb_channel_layouts_use_drm_fourcc_not_spa_enum_values() {
        for (video, bytes) in [
            (VideoFormat::BGRA, *b"AR24"),
            (VideoFormat::BGRx, *b"XR24"),
            (VideoFormat::RGBA, *b"AB24"),
            (VideoFormat::RGBx, *b"XB24"),
        ] {
            assert_eq!(fourcc(video).unwrap().to_le_bytes(), bytes);
        }
        assert!(fourcc(VideoFormat::NV12).is_err());
    }

    #[test]
    fn auxiliary_planes_preserve_modifier_bits_and_implicit_is_not_linear() {
        let plane = Plane {
            fd: 9,
            offset: 4096,
            stride: 512,
        };
        let format = Format {
            video: VideoFormat::BGRx,
            modifier: 0x0380_0000_ffff_ffff,
        };
        let attrs = attributes(format, 100, 60, &[plane; 4]).unwrap();
        let pairs = attrs[..attrs.len() - 1]
            .chunks_exact(2)
            .map(|p| (p[0], p[1]))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(pairs[&0x3440], 9); // fourth auxiliary plane
        assert_eq!(pairs[&0x30D2], 1);
        assert_eq!(pairs[&0x3449], -1);
        assert_eq!(pairs[&0x344A], 0x0380_0000);
        let implicit = attributes(
            Format {
                modifier: IMPLICIT_MODIFIER,
                ..format
            },
            100,
            60,
            &[plane],
        )
        .unwrap();
        assert!(
            !implicit[..implicit.len() - 1]
                .chunks_exact(2)
                .any(|p| p[0] == 0x3443)
        );
        let linear = attributes(
            Format {
                modifier: 0,
                ..format
            },
            100,
            60,
            &[plane],
        )
        .unwrap();
        assert!(
            linear[..linear.len() - 1]
                .chunks_exact(2)
                .any(|p| p == [0x3443, 0])
        );
    }

    #[test]
    fn rejects_unsafe_import_attributes_before_calling_the_driver() {
        let format = Format {
            video: VideoFormat::RGBA,
            modifier: 0,
        };
        let plane = Plane {
            fd: 3,
            offset: 0,
            stride: 12,
        };
        for (w, h) in [(0, 2), (2, 0), (u32::MAX, 1), (8192, 8192)] {
            assert!(attributes(format, w, h, &[plane]).is_err());
        }
        for planes in [
            vec![],
            vec![plane; 5],
            vec![Plane { fd: -1, ..plane }],
            vec![Plane {
                offset: -1,
                ..plane
            }],
            vec![Plane {
                stride: -12,
                ..plane
            }],
        ] {
            assert!(attributes(format, 3, 2, &planes).is_err());
        }
    }

    #[test]
    fn gpu_planes_do_not_require_cpu_mappings_and_check_offsets_without_fd_truncation() {
        use pipewire::spa::sys::*;
        let mut chunk = spa_chunk {
            offset: 12,
            size: 0,
            stride: 128,
            flags: 0,
        };
        let mut raw = spa_data {
            type_: SPA_DATA_DmaBuf,
            flags: SPA_DATA_FLAG_READABLE,
            fd: 7,
            mapoffset: 4096,
            maxsize: 0,
            data: ptr::null_mut(),
            chunk: &mut chunk,
        };
        fn decode(raw: &spa_data) -> Result<Vec<Plane>, CaptureError> {
            // SAFETY: libspa's Data is repr(transparent) over spa_data; the
            // descriptor/chunk remain alive for this borrowed call.
            planes(std::slice::from_ref(unsafe {
                &*(raw as *const spa_data).cast::<Data>()
            }))
        }
        assert_eq!(
            decode(&raw).unwrap(),
            vec![Plane {
                fd: 7,
                offset: 4108,
                stride: 128
            }]
        );
        raw.fd = i64::MAX;
        assert!(decode(&raw).is_err());
        raw.fd = 7;
        raw.mapoffset = u32::MAX;
        assert!(decode(&raw).is_err());
        raw.mapoffset = 0;
        raw.type_ = SPA_DATA_MemFd;
        assert!(decode(&raw).is_err());
        raw.type_ = SPA_DATA_DmaBuf;
        raw.chunk = ptr::null_mut();
        assert!(decode(&raw).is_err());
    }

    #[test]
    #[ignore = "run scripts/check-gpu-capture.sh for isolated software EGL readback"]
    fn software_egl_readback_preserves_pixels_and_recovers_after_cancelled_and_invalid_imports() {
        assert_eq!(
            std::env::var("PROLLYGLOT_PRIVATE_GPU_TEST").as_deref(),
            Ok("1")
        );
        assert_eq!(std::env::var("LIBGL_ALWAYS_SOFTWARE").as_deref(), Ok("1"));
        for name in ["DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET"] {
            assert!(std::env::var_os(name).is_none());
        }
        let egl = Rc::new(unsafe { Egl::load_required() }.unwrap());
        // Explicit software surfaceless display: no device or desktop server.
        let display =
            unsafe { egl.get_platform_display(0x31DD, ptr::null_mut(), &[egl::ATTRIB_NONE]) }
                .unwrap();
        let device = Device::open(Session::open(egl, display).unwrap(), false).unwrap();
        let pixels = (1..=6u8)
            .flat_map(|n| [n * 10, n * 20, n * 30, 255])
            .collect::<Vec<_>>();
        for index in 0..12 {
            let mut objects = Objects::new(&device);
            unsafe {
                objects.texture = Some(device.gl.create_texture().unwrap());
                device.gl.bind_texture(glow::TEXTURE_2D, objects.texture);
                device.gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    3,
                    2,
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(Some(&pixels)),
                );
            }
            if index % 3 == 0 {
                assert!(matches!(
                    objects.read(3, 2, &AtomicBool::new(true)),
                    Err(CaptureError::Cancelled)
                ));
            } else {
                assert_eq!(objects.read(3, 2, &AtomicBool::new(false)).unwrap(), pixels);
            }
        }
        let fd = std::fs::File::open("/dev/null").unwrap();
        assert!(
            device
                .read(
                    Format {
                        video: VideoFormat::RGBA,
                        modifier: 0
                    },
                    3,
                    2,
                    &[Plane {
                        fd: fd.as_raw_fd(),
                        offset: 0,
                        stride: 12
                    }],
                    &AtomicBool::new(false)
                )
                .is_err()
        );
        // Failed import must not consume the caller's descriptor.
        assert!(fd.metadata().is_ok());
    }
}
