use crate::{PixelRect, VisualPipelineError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Bgra8,
}

impl PixelFormat {
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Bgra8 => 4,
        }
    }
}

/// A transient CPU frame. This type deliberately does not implement Serialize:
/// captured pixels must never cross IPC or enter diagnostics by accident.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisualFrame {
    pub sequence: u64,
    pub captured_at_micros: u64,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub pixel_format: PixelFormat,
    pixels: Vec<u8>,
}

impl VisualFrame {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sequence: u64,
        captured_at_micros: u64,
        width: u32,
        height: u32,
        stride: usize,
        pixel_format: PixelFormat,
        pixels: Vec<u8>,
    ) -> Result<Self, VisualPipelineError> {
        if width == 0 || height == 0 {
            return Err(VisualPipelineError::InvalidFrame(
                "frame dimensions must be non-zero".into(),
            ));
        }
        let minimum_stride = usize::try_from(width)
            .ok()
            .and_then(|value| value.checked_mul(pixel_format.bytes_per_pixel()))
            .ok_or_else(|| VisualPipelineError::InvalidFrame("frame stride overflowed".into()))?;
        if stride < minimum_stride {
            return Err(VisualPipelineError::InvalidFrame(format!(
                "frame stride {stride} is smaller than {minimum_stride}"
            )));
        }
        let expected = stride
            .checked_mul(usize::try_from(height).unwrap_or(usize::MAX))
            .ok_or_else(|| {
                VisualPipelineError::InvalidFrame("frame byte length overflowed".into())
            })?;
        if pixels.len() != expected {
            return Err(VisualPipelineError::InvalidFrame(format!(
                "frame contains {} bytes; expected {expected}",
                pixels.len()
            )));
        }
        Ok(Self {
            sequence,
            captured_at_micros,
            width,
            height,
            stride,
            pixel_format,
            pixels,
        })
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn crop(&self, region: PixelRect) -> Result<Self, VisualPipelineError> {
        if !region.fits_within(self.width, self.height) {
            return Err(VisualPipelineError::InvalidCrop);
        }
        let bytes_per_pixel = self.pixel_format.bytes_per_pixel();
        let cropped_stride = usize::try_from(region.width)
            .unwrap_or(usize::MAX)
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| VisualPipelineError::InvalidFrame("crop stride overflowed".into()))?;
        let cropped_length = cropped_stride
            .checked_mul(usize::try_from(region.height).unwrap_or(usize::MAX))
            .ok_or_else(|| {
                VisualPipelineError::InvalidFrame("crop byte length overflowed".into())
            })?;
        let mut pixels = vec![0_u8; cropped_length];
        let source_x = usize::try_from(region.x).unwrap_or(usize::MAX) * bytes_per_pixel;
        for row in 0..usize::try_from(region.height).unwrap_or(0) {
            let source_y = usize::try_from(region.y).unwrap_or(usize::MAX) + row;
            let source_start = source_y * self.stride + source_x;
            let target_start = row * cropped_stride;
            pixels[target_start..target_start + cropped_stride]
                .copy_from_slice(&self.pixels[source_start..source_start + cropped_stride]);
        }
        Self::new(
            self.sequence,
            self.captured_at_micros,
            region.width,
            region.height,
            cropped_stride,
            self.pixel_format,
            pixels,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_by_two() -> VisualFrame {
        VisualFrame::new(
            1,
            0,
            2,
            2,
            8,
            PixelFormat::Bgra8,
            vec![
                1, 2, 3, 4, 5, 6, 7, 8, // row one
                9, 10, 11, 12, 13, 14, 15, 16, // row two
            ],
        )
        .expect("valid frame")
    }

    #[test]
    fn rejects_short_rows() {
        assert!(VisualFrame::new(0, 0, 2, 2, 7, PixelFormat::Bgra8, vec![0; 14]).is_err());
    }

    #[test]
    fn crops_without_retaining_unselected_pixels() {
        let cropped = two_by_two()
            .crop(PixelRect {
                x: 1,
                y: 0,
                width: 1,
                height: 2,
            })
            .expect("crop");
        assert_eq!(cropped.width, 1);
        assert_eq!(cropped.stride, 4);
        assert_eq!(cropped.pixels(), &[5, 6, 7, 8, 13, 14, 15, 16]);
    }
}
