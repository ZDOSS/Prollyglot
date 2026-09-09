//! Portal compositor coordinates and normalized capture pixels are distinct.
//! Only a unique, exact logical monitor match can authorize an X11 overlay.

use prollyglot_visual_pipeline::PixelRect;
use prollyglot_visual_pipewire::{FrameGeometry, PortalSource, StreamInfo};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogicalRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Clone, Debug)]
pub struct PortalGeometry {
    pub stream: StreamInfo,
    pub frame: FrameGeometry,
    pub region: Option<PixelRect>,
}

impl PortalGeometry {
    pub fn anchor(&self, monitors: &[LogicalRect]) -> Option<LogicalRect> {
        if self.stream.source != PortalSource::Monitor {
            return None;
        }
        let (x, y) = self.stream.logical_position?;
        let (width, height) = self.stream.logical_size?;
        let monitor = LogicalRect {
            x,
            y,
            width: i32::try_from(width).ok()?,
            height: i32::try_from(height).ok()?,
        };
        if width == 0 || height == 0 || monitors.iter().filter(|m| **m == monitor).count() != 1 {
            return None;
        }
        // A partial buffer crop does not describe a whole monitor. Rotation is
        // already applied by the backend; use normalized dimensions only.
        if self.frame.crop != PixelRect::full(self.frame.raw_width, self.frame.raw_height)
            || self.frame.width == 0
            || self.frame.height == 0
        {
            return None;
        }
        // Allow one pixel of compositor rounding, but never stretch a stream
        // whose aspect ratio contradicts the reported logical monitor.
        let cross = (i64::from(self.frame.width) * i64::from(height)
            - i64::from(self.frame.height) * i64::from(width))
        .abs();
        if cross > i64::from(width.max(height)) {
            return None;
        }
        let region = self
            .region
            .unwrap_or(PixelRect::full(self.frame.width, self.frame.height));
        if !region.fits_within(self.frame.width, self.frame.height) {
            return None;
        }
        let scale = |pixel: u32, logical: u32, pixels: u32| -> Option<i32> {
            i32::try_from(
                (u64::from(pixel) * u64::from(logical) + u64::from(pixels) / 2) / u64::from(pixels),
            )
            .ok()
        };
        let left = scale(region.x, width, self.frame.width)?;
        let top = scale(region.y, height, self.frame.height)?;
        let right = scale(region.x + region.width, width, self.frame.width)?;
        let bottom = scale(region.y + region.height, height, self.frame.height)?;
        (right > left && bottom > top).then_some(LogicalRect {
            x: x.checked_add(left)?,
            y: y.checked_add(top)?,
            width: right - left,
            height: bottom - top,
        })
    }
}

/// Map a drag inside a letterboxed preview back to capture pixels. A drag
/// starting in a letterbox is ignored instead of selecting unrelated pixels.
pub fn preview_point(
    width: u32,
    height: u32,
    area: (f64, f64),
    point: (f64, f64),
    clamp: bool,
) -> Option<(f64, f64)> {
    if width == 0
        || height == 0
        || area.0 <= 0.0
        || area.1 <= 0.0
        || ![area.0, area.1, point.0, point.1]
            .iter()
            .all(|n| n.is_finite())
    {
        return None;
    }
    let scale = (area.0 / f64::from(width)).min(area.1 / f64::from(height));
    let x = (point.0 - (area.0 - f64::from(width) * scale) / 2.0) / scale;
    let y = (point.1 - (area.1 - f64::from(height) * scale) / 2.0) / scale;
    if !clamp && (x < 0.0 || y < 0.0 || x > f64::from(width) || y > f64::from(height)) {
        return None;
    }
    Some((
        x.clamp(0.0, f64::from(width)),
        y.clamp(0.0, f64::from(height)),
    ))
}

pub fn drag_region(start: (f64, f64), end: (f64, f64)) -> PixelRect {
    let x = start.0.min(end.0).floor() as u32;
    let y = start.1.min(end.1).floor() as u32;
    PixelRect {
        x,
        y,
        width: start.0.max(end.0).ceil() as u32 - x,
        height: start.1.max(end.1).ceil() as u32 - y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn geometry(scale: u32) -> PortalGeometry {
        PortalGeometry {
            stream: StreamInfo {
                source: PortalSource::Monitor,
                logical_position: Some((-1280, 120)),
                logical_size: Some((1280, 720)),
            },
            frame: FrameGeometry {
                raw_width: 1280 * scale,
                raw_height: 720 * scale,
                crop: PixelRect::full(1280 * scale, 720 * scale),
                transform: 0,
                width: 1280 * scale,
                height: 720 * scale,
            },
            region: None,
        }
    }
    fn monitor() -> LogicalRect {
        LogicalRect {
            x: -1280,
            y: 120,
            width: 1280,
            height: 720,
        }
    }

    #[test]
    fn maps_negative_origins_and_scaled_regions_in_logical_coordinates() {
        for scale in [1, 2, 3] {
            let mut g = geometry(scale);
            assert_eq!(g.anchor(&[monitor()]), Some(monitor()));
            g.region = Some(PixelRect {
                x: 320 * scale,
                y: 180 * scale,
                width: 640 * scale,
                height: 360 * scale,
            });
            assert_eq!(
                g.anchor(&[monitor()]),
                Some(LogicalRect {
                    x: -960,
                    y: 300,
                    width: 640,
                    height: 360
                })
            );
        }
        let mut g = geometry(1);
        g.frame = FrameGeometry {
            raw_width: 1600,
            raw_height: 900,
            crop: PixelRect::full(1600, 900),
            width: 1600,
            height: 900,
            transform: 0,
        };
        assert_eq!(
            g.anchor(&[monitor()]),
            Some(monitor()),
            "fractional buffer scale"
        );
        g.frame.raw_width = 900;
        g.frame.raw_height = 1600;
        g.frame.crop = PixelRect::full(900, 1600);
        g.frame.transform = 1;
        assert_eq!(
            g.anchor(&[monitor()]),
            Some(monitor()),
            "rotation already normalized"
        );
    }

    #[test]
    fn rejects_missing_ambiguous_window_and_contradictory_geometry() {
        let mut g = geometry(2);
        assert_eq!(g.anchor(&[]), None);
        assert_eq!(g.anchor(&[monitor(), monitor()]), None);
        g.stream.source = PortalSource::Window;
        assert_eq!(g.anchor(&[monitor()]), None);
        g.stream.source = PortalSource::Monitor;
        g.frame.width = 720;
        assert_eq!(g.anchor(&[monitor()]), None);
        g = geometry(1);
        g.frame.crop.x = 1;
        assert_eq!(g.anchor(&[monitor()]), None);
        g = geometry(1);
        g.stream.logical_position = None;
        assert_eq!(g.anchor(&[monitor()]), None);
    }

    #[test]
    fn preview_letterboxing_reverse_drag_and_edges() {
        assert_eq!(
            preview_point(1920, 1080, (1000.0, 800.0), (0.0, 0.0), false),
            None
        );
        let start = preview_point(1920, 1080, (1000.0, 800.0), (750.0, 540.625), false).unwrap();
        let end = preview_point(1920, 1080, (1000.0, 800.0), (250.0, 259.375), false).unwrap();
        assert_eq!(
            drag_region(start, end),
            PixelRect {
                x: 480,
                y: 270,
                width: 960,
                height: 540
            }
        );
        assert_eq!(
            preview_point(1920, 1080, (1000.0, 800.0), (-10.0, 1000.0), true),
            Some((0.0, 1080.0))
        );
    }
}
