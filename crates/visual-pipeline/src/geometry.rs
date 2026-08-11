use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    pub const fn full(width: u32, height: u32) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    pub fn fits_within(self, width: u32, height: u32) -> bool {
        self.width > 0
            && self.height > 0
            && self
                .x
                .checked_add(self.width)
                .is_some_and(|right| right <= width)
            && self
                .y
                .checked_add(self.height)
                .is_some_and(|bottom| bottom <= height)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl VisualRect {
    pub fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.x >= 0.0
            && self.y >= 0.0
            && self.width > 0.0
            && self.height > 0.0
    }

    pub fn area(self) -> f32 {
        if !self.is_valid() {
            return 0.0;
        }
        self.width * self.height
    }

    pub fn intersection_over_union(self, other: Self) -> f32 {
        if !self.is_valid() || !other.is_valid() {
            return 0.0;
        }
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        let intersection = (right - left).max(0.0) * (bottom - top).max(0.0);
        let union = self.area() + other.area() - intersection;
        if union <= f32::EPSILON {
            0.0
        } else {
            intersection / union
        }
    }

    pub fn smoothed_toward(self, next: Self, weight: f32) -> Self {
        let weight = weight.clamp(0.0, 1.0);
        let retain = 1.0 - weight;
        Self {
            x: self.x * retain + next.x * weight,
            y: self.y * retain + next.y * weight,
            width: self.width * retain + next.width * weight,
            height: self.height * retain + next.height * weight,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_crop_must_fit_inside_frame() {
        assert!(
            PixelRect {
                x: 10,
                y: 20,
                width: 30,
                height: 40,
            }
            .fits_within(100, 100)
        );
        assert!(
            !PixelRect {
                x: 90,
                y: 20,
                width: 30,
                height: 40,
            }
            .fits_within(100, 100)
        );
    }

    #[test]
    fn overlap_is_scale_independent() {
        let first = VisualRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
        };
        let shifted = VisualRect {
            x: 10.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
        };
        assert!((first.intersection_over_union(shifted) - (90.0 / 110.0)).abs() < 0.001);
    }
}
