//! Opt-in synthetic DMA-BUF check. No portal, desktop, media, or display modes.
use super::*;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

type CreateDevice = unsafe extern "C" fn(i32) -> *mut c_void;
type CreateBo = unsafe extern "C" fn(*mut c_void, u32, u32, u32, u32) -> *mut c_void;
type Destroy = unsafe extern "C" fn(*mut c_void);
type Map = unsafe extern "C" fn(
    *mut c_void,
    u32,
    u32,
    u32,
    u32,
    u32,
    *mut u32,
    *mut *mut c_void,
) -> *mut c_void;
type Unmap = unsafe extern "C" fn(*mut c_void, *mut c_void);
type GetFd = unsafe extern "C" fn(*mut c_void) -> i32;
type GetStride = unsafe extern "C" fn(*mut c_void) -> u32;
type GetModifier = unsafe extern "C" fn(*mut c_void) -> u64;

struct Allocation {
    device: *mut c_void,
    bo: *mut c_void,
    destroy_bo: Destroy,
    destroy_device: Destroy,
    _library: libloading::Library,
}
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            if !self.bo.is_null() {
                (self.destroy_bo)(self.bo);
            }
            (self.destroy_device)(self.device);
        }
    }
}

#[test]
#[ignore = "requires an explicitly supplied DRM render node; run scripts/check-gpu-capture.sh"]
fn real_dmabuf_import_preserves_channel_and_row_order() {
    for name in ["DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET"] {
        assert!(std::env::var_os(name).is_none());
    }
    let node = std::env::var("PROLLYGLOT_DMABUF_RENDER_NODE")
        .expect("Supply /dev/dri/renderD… explicitly");
    let suffix = node
        .strip_prefix("/dev/dri/renderD")
        .expect("Only a DRM render node is allowed");
    assert!(suffix.parse::<u32>().is_ok_and(|n| n >= 128));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(node)
        .unwrap();
    // SAFETY: libgbm's stable C ABI; the library, device and allocation remain
    // owned until after import/readback, and the export FD is independently owned.
    unsafe {
        let library = libloading::Library::new("libgbm.so.1").unwrap();
        let create: CreateDevice = *library.get(b"gbm_create_device\0").unwrap();
        let create_bo: CreateBo = *library.get(b"gbm_bo_create\0").unwrap();
        let map: Map = *library.get(b"gbm_bo_map\0").unwrap();
        let unmap: Unmap = *library.get(b"gbm_bo_unmap\0").unwrap();
        let get_fd: GetFd = *library.get(b"gbm_bo_get_fd\0").unwrap();
        let get_stride: GetStride = *library.get(b"gbm_bo_get_stride\0").unwrap();
        let get_modifier: GetModifier = *library.get(b"gbm_bo_get_modifier\0").unwrap();
        let device = create(file.as_raw_fd());
        assert!(!device.is_null(), "GBM device creation failed");
        let mut allocation = Allocation {
            device,
            bo: ptr::null_mut(),
            destroy_bo: *library.get(b"gbm_bo_destroy\0").unwrap(),
            destroy_device: *library.get(b"gbm_device_destroy\0").unwrap(),
            _library: library,
        };
        let video = VideoFormat::BGRx;
        allocation.bo = create_bo(
            device,
            4,
            3,
            fourcc(video).unwrap() as u32,
            (1 << 2) | (1 << 4),
        );
        assert!(
            !allocation.bo.is_null(),
            "A linear RGB GBM allocation is unavailable"
        );
        let mut stride = 0;
        let mut map_data = ptr::null_mut();
        let mapped = map(allocation.bo, 0, 0, 4, 3, 2, &mut stride, &mut map_data);
        assert!(
            !mapped.is_null() && stride >= 16,
            "GBM write mapping failed"
        );
        let mut expected = Vec::new();
        for y in 0..3usize {
            for x in 0..4usize {
                let (b, g, r) = (
                    (x * 40 + 5) as u8,
                    (y * 70 + 7) as u8,
                    (x * 10 + y * 20 + 9) as u8,
                );
                ptr::copy_nonoverlapping(
                    [b, g, r, 0].as_ptr(),
                    mapped.cast::<u8>().add(y * stride as usize + x * 4),
                    4,
                );
                expected.extend([r, g, b, 255]);
            }
        }
        unmap(allocation.bo, map_data);
        let fd = get_fd(allocation.bo);
        assert!(fd >= 0, "GBM did not export a DMA-BUF");
        let fd = OwnedFd::from_raw_fd(fd);
        let format = Format {
            video,
            modifier: get_modifier(allocation.bo),
        };
        let importer = Importer::open().unwrap();
        let plane = Plane {
            fd: fd.as_raw_fd(),
            offset: 0,
            stride: i32::try_from(get_stride(allocation.bo)).unwrap(),
        };
        assert_eq!(
            importer
                .read(format, 4, 3, &[plane], &AtomicBool::new(false))
                .unwrap(),
            expected
        );
        // Repeat on the same producer FD to detect accidental import ownership.
        assert_eq!(
            importer
                .read(format, 4, 3, &[plane], &AtomicBool::new(false))
                .unwrap(),
            expected
        );
    }
}
