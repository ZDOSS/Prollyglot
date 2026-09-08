//! Preserve small desktop text before detector downscaling. Contrast proposes
//! areas, never words or languages; the OCR model still decides what is text.
//! Busy areas use overlapping tiles with a fair, bounded scan. Cached lines
//! survive only while their actual source pixels remain identical.

use std::collections::VecDeque;

use prollyglot_visual_pipeline::{OcrObservation, PixelRect, VisualFrame, VisualRect};
use rapidocr_core::OcrCancellationToken;

const CELL: u32 = 32;
const MARGIN: u32 = 32;
const TILE_WIDTH: u32 = 960;
const TILE_HEIGHT: u32 = 544;
const OVERLAP: u32 = 160;
pub(crate) const PIXEL_BUDGET: u64 = 700_000;

struct Region {
    bounds: PixelRect,
    lines: Vec<OcrObservation>,
    dirty: bool,
    last_scan: u64,
    had_text: bool,
}

#[derive(Default)]
pub(crate) struct RegionScanner {
    previous: Option<VisualFrame>,
    regions: Vec<Region>,
    pass: u64,
}

impl RegionScanner {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn pending(&self) -> bool {
        self.regions.iter().any(|region| region.dirty)
    }

    pub fn prepare(
        &mut self,
        frame: &VisualFrame,
        cancellation: &OcrCancellationToken,
    ) -> Result<(), prollyglot_visual_pipeline::OcrError> {
        self.pass = self.pass.saturating_add(1);
        if self.previous.as_ref().is_some_and(|old| {
            old.width == frame.width
                && old.height == frame.height
                && old.stride == frame.stride
                && old.pixels() == frame.pixels()
        }) {
            return Ok(());
        }
        let plan = discover(frame, cancellation)?;
        let mut old_regions = std::mem::take(&mut self.regions);
        self.regions = plan
            .into_iter()
            .map(|bounds| {
                let Some(index) = old_regions.iter().position(|old| old.bounds == bounds) else {
                    return Region {
                        bounds,
                        lines: Vec::new(),
                        dirty: true,
                        last_scan: 0,
                        had_text: false,
                    };
                };
                let mut region = old_regions.swap_remove(index);
                if !self
                    .previous
                    .as_ref()
                    .is_some_and(|old| same_pixels(old, frame, bounds))
                {
                    region.dirty = true;
                    region.lines.retain(|line| {
                        self.previous.as_ref().is_some_and(|old| {
                            same_pixels(old, frame, pixel_bounds(line.bounds, frame, 2))
                        })
                    });
                }
                region
            })
            .collect();
        self.previous = Some(frame.clone());
        Ok(())
    }

    pub fn work(&self) -> Vec<(usize, PixelRect)> {
        let mut work: Vec<_> = self
            .regions
            .iter()
            .enumerate()
            .filter(|(_, r)| r.dirty)
            .collect();
        // Every second pass reserves the first slot for a known text area.
        // The intervening oldest-first pass guarantees discovery progress.
        let frame = self.previous.as_ref();
        let priority = |r: &Region| {
            let center_distance = frame.map_or(0, |f| {
                let dx = (r.bounds.x + r.bounds.width / 2).abs_diff(f.width / 2);
                let dy = (r.bounds.y + r.bounds.height / 2).abs_diff(f.height * 9 / 10);
                dx + dy
            });
            (
                r.last_scan,
                area(r.bounds) > 300_000,
                center_distance,
                area(r.bounds),
            )
        };
        work.sort_by_key(|(_, r)| priority(r));
        if self.pass % 2 == 1
            && let Some(index) = work.iter().position(|(_, r)| r.had_text)
        {
            let hot = work.remove(index);
            work.insert(0, hot);
        }
        work.into_iter()
            .map(|(index, r)| (index, r.bounds))
            .collect()
    }

    pub fn complete(&mut self, index: usize, lines: Vec<OcrObservation>) {
        let region = &mut self.regions[index];
        region.lines = lines;
        region.had_text = region
            .lines
            .iter()
            .any(|line| line.text.chars().any(char::is_alphabetic));
        region.dirty = false;
        region.last_scan = self.pass;
    }

    pub fn observations(&self) -> Vec<OcrObservation> {
        let mut lines: Vec<_> = self
            .regions
            .iter()
            .flat_map(|r| r.lines.iter().cloned())
            .collect();
        // Prefer the complete line when an overlap also recognized a fragment.
        lines.sort_by(|a, b| {
            b.text
                .chars()
                .count()
                .cmp(&a.text.chars().count())
                .then_with(|| b.confidence.total_cmp(&a.confidence))
        });
        let mut unique: Vec<OcrObservation> = Vec::new();
        for line in lines {
            if !unique.iter().any(|other| duplicates(other, &line)) {
                unique.push(line);
            }
        }
        unique
    }
}

pub(crate) fn area(bounds: PixelRect) -> u64 {
    u64::from(bounds.width) * u64::from(bounds.height)
}

fn duplicates(a: &OcrObservation, b: &OcrObservation) -> bool {
    let overlap =
        (a.bounds.x + a.bounds.width).min(b.bounds.x + b.bounds.width) - a.bounds.x.max(b.bounds.x);
    let vertical = (a.bounds.y + a.bounds.height).min(b.bounds.y + b.bounds.height)
        - a.bounds.y.max(b.bounds.y);
    let coverage =
        overlap.max(0.0) * vertical.max(0.0) / a.bounds.area().min(b.bounds.area()).max(1.0);
    let a_text = a.normalized_text();
    let b_text = b.normalized_text();
    coverage >= 0.7
        && (a_text == b_text || (b_text.chars().count() >= 3 && a_text.contains(&b_text)))
}

fn same_pixels(old: &VisualFrame, next: &VisualFrame, bounds: PixelRect) -> bool {
    if old.width != next.width
        || old.height != next.height
        || !bounds.fits_within(next.width, next.height)
    {
        return false;
    }
    let left = bounds.x as usize * 4;
    let width = bounds.width as usize * 4;
    (bounds.y..bounds.y + bounds.height).all(|y| {
        let a = y as usize * old.stride + left;
        let b = y as usize * next.stride + left;
        old.pixels()[a..a + width] == next.pixels()[b..b + width]
    })
}

fn pixel_bounds(bounds: VisualRect, frame: &VisualFrame, margin: u32) -> PixelRect {
    let x = (bounds.x as u32)
        .saturating_sub(margin)
        .min(frame.width - 1);
    let y = (bounds.y as u32)
        .saturating_sub(margin)
        .min(frame.height - 1);
    let right = (bounds.x + bounds.width).ceil() as u32;
    let bottom = (bounds.y + bounds.height).ceil() as u32;
    PixelRect {
        x,
        y,
        width: right
            .saturating_add(margin)
            .min(frame.width)
            .saturating_sub(x)
            .max(1),
        height: bottom
            .saturating_add(margin)
            .min(frame.height)
            .saturating_sub(y)
            .max(1),
    }
}

fn padded(x: u32, y: u32, right: u32, bottom: u32, margin: u32, frame: &VisualFrame) -> PixelRect {
    let left = x.saturating_sub(margin);
    let top = y.saturating_sub(margin);
    PixelRect {
        x: left,
        y: top,
        width: right.saturating_add(margin).min(frame.width) - left,
        height: bottom.saturating_add(margin).min(frame.height) - top,
    }
}

fn discover(
    frame: &VisualFrame,
    cancellation: &OcrCancellationToken,
) -> Result<Vec<PixelRect>, prollyglot_visual_pipeline::OcrError> {
    if frame.width <= 1_280 && frame.height <= 864 {
        return Ok(vec![PixelRect::full(frame.width, frame.height)]);
    }
    let columns = frame.width.div_ceil(CELL) as usize;
    let rows = frame.height.div_ceil(CELL) as usize;
    let mut active = vec![false; columns * rows];
    for cy in 0..rows {
        cancellation
            .checkpoint()
            .map_err(|_| prollyglot_visual_pipeline::OcrError::Cancelled)?;
        for cx in 0..columns {
            let left = cx as u32 * CELL;
            let top = cy as u32 * CELL;
            let origin = top as usize * frame.stride + left as usize * 4;
            let reference = &frame.pixels()[origin..origin + 3];
            'pixels: for y in top..(top + CELL).min(frame.height) {
                let start = y as usize * frame.stride + left as usize * 4;
                let end = y as usize * frame.stride + (left + CELL).min(frame.width) as usize * 4;
                for pixel in frame.pixels()[start..end].chunks_exact(4) {
                    if (0..3).any(|c| pixel[c].abs_diff(reference[c]) >= 12) {
                        active[cy * columns + cx] = true;
                        break 'pixels;
                    }
                }
            }
        }
    }
    // Bridge a blank cell between words without joining distant UI panels.
    let original = active.clone();
    for row in 0..rows {
        for col in 1..columns.saturating_sub(1) {
            if original[row * columns + col - 1] && original[row * columns + col + 1] {
                active[row * columns + col] = true;
            }
        }
    }
    let mut regions = Vec::new();
    for start in 0..active.len() {
        if !active[start] {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        active[start] = false;
        let (mut left, mut right, mut top, mut bottom) = (columns, 0, rows, 0);
        while let Some(index) = queue.pop_front() {
            let (x, y) = (index % columns, index / columns);
            left = left.min(x);
            right = right.max(x + 1);
            top = top.min(y);
            bottom = bottom.max(y + 1);
            for ny in y.saturating_sub(1)..=(y + 1).min(rows - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(columns - 1) {
                    let neighbor = ny * columns + nx;
                    if active[neighbor] {
                        active[neighbor] = false;
                        queue.push_back(neighbor);
                    }
                }
            }
        }
        let bounds = padded(
            left as u32 * CELL,
            top as u32 * CELL,
            right as u32 * CELL,
            bottom as u32 * CELL,
            MARGIN,
            frame,
        );
        if bounds.width <= 1_280 && bounds.height <= 864 {
            regions.push(bounds);
        } else {
            // Fixed grid anchors keep scheduling/cache identities stable when
            // a large connected background changes its contour.
            for y in (bounds.y / TILE_HEIGHT * TILE_HEIGHT..bounds.y + bounds.height)
                .step_by(TILE_HEIGHT as usize)
            {
                for x in (bounds.x / TILE_WIDTH * TILE_WIDTH..bounds.x + bounds.width)
                    .step_by(TILE_WIDTH as usize)
                {
                    let tile = padded(
                        x,
                        y,
                        (x + TILE_WIDTH).min(frame.width),
                        (y + TILE_HEIGHT).min(frame.height),
                        OVERLAP,
                        frame,
                    );
                    if !regions.contains(&tile) {
                        regions.push(tile);
                    }
                }
            }
        }
    }
    Ok(regions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use prollyglot_visual_pipeline::PixelFormat;

    fn frame(width: u32, height: u32, marks: &[PixelRect]) -> VisualFrame {
        let mut pixels = vec![0; width as usize * height as usize * 4];
        for mark in marks {
            for y in mark.y..mark.y + mark.height {
                for x in mark.x..mark.x + mark.width {
                    let index = (y * width + x) as usize * 4;
                    pixels[index..index + 3].fill(if x % 4 < 2 { 255 } else { 0 });
                }
            }
        }
        VisualFrame::new(
            1,
            0,
            width,
            height,
            width as usize * 4,
            PixelFormat::Bgra8,
            pixels,
        )
        .unwrap()
    }

    fn line(x: f32, y: f32, text: &str) -> OcrObservation {
        OcrObservation {
            text: text.into(),
            confidence: 0.98,
            language: None,
            script: None,
            bounds: VisualRect {
                x,
                y,
                width: 120.0,
                height: 24.0,
            },
        }
    }

    #[test]
    fn discovers_small_text_at_edges_and_across_grid_boundaries_on_4k() {
        for (x, y) in [(0, 0), (3700, 2110), (910, 520), (1810, 2010)] {
            let mark = PixelRect {
                x,
                y,
                width: 120,
                height: 24,
            };
            let frame = frame(3840, 2160, &[mark]);
            let plan = discover(&frame, &OcrCancellationToken::new()).unwrap();
            assert_eq!(plan.len(), 1);
            let area = plan[0];
            assert!(area.x <= x && area.y <= y);
            assert!(area.x + area.width >= x + mark.width);
            assert!(area.y + area.height >= y + mark.height);
            assert!(area.width < 256 && area.height < 160);
        }
    }

    #[test]
    fn cache_keeps_only_current_pixels_and_resets_on_resize() {
        let mark = PixelRect {
            x: 0,
            y: 0,
            width: 120,
            height: 24,
        };
        let first = frame(640, 360, &[mark]);
        let token = OcrCancellationToken::new();
        let mut scanner = RegionScanner::default();
        scanner.prepare(&first, &token).unwrap();
        scanner.complete(0, vec![line(0.0, 0.0, "Buenos días")]);
        scanner.prepare(&first, &token).unwrap();
        assert!(!scanner.pending());
        assert_eq!(scanner.observations().len(), 1);
        let background = PixelRect {
            x: 300,
            y: 200,
            width: 120,
            height: 24,
        };
        scanner
            .prepare(&frame(640, 360, &[mark, background]), &token)
            .unwrap();
        assert!(scanner.pending());
        assert_eq!(scanner.observations().len(), 1);
        scanner.prepare(&frame(640, 360, &[]), &token).unwrap();
        assert!(scanner.observations().is_empty());
        scanner.prepare(&frame(800, 600, &[mark]), &token).unwrap();
        assert!(scanner.observations().is_empty());
        assert_eq!(scanner.work()[0].1, PixelRect::full(800, 600));
    }

    #[test]
    fn dense_display_is_bounded_and_oldest_tiles_cannot_starve() {
        let dense = frame(3840, 2160, &[PixelRect::full(3840, 2160)]);
        let token = OcrCancellationToken::new();
        let mut scanner = RegionScanner::default();
        scanner.prepare(&dense, &token).unwrap();
        let count = scanner.work().len();
        assert_eq!(count, 16);
        let mut visited = Vec::new();
        for _ in 0..count {
            let (index, bounds) = scanner.work()[0];
            assert!(bounds.width <= 1280 && bounds.height <= 864);
            assert!(bounds.fits_within(3840, 2160));
            assert!(!visited.contains(&index));
            visited.push(index);
            scanner.complete(index, vec![]);
            // Re-dirty an already visited area as a moving video would.
            scanner.regions[index].dirty = true;
            scanner.prepare(&dense, &token).unwrap();
        }
    }

    #[test]
    fn overlapping_crops_deduplicate_without_hiding_identical_text_elsewhere() {
        let mut a = line(100.0, 100.0, "Buenos días");
        let mut b = line(130.0, 100.0, "días");
        b.bounds.width = 90.0;
        assert!(duplicates(&a, &b));
        a.bounds.x = 400.0;
        assert!(!duplicates(&a, &b));
    }

    #[test]
    fn changing_known_text_gets_priority_without_starving_discovery() {
        let dense = frame(3840, 2160, &[PixelRect::full(3840, 2160)]);
        let token = OcrCancellationToken::new();
        let mut scanner = RegionScanner::default();
        scanner.prepare(&dense, &token).unwrap();
        let count = scanner.work().len();
        let hot = scanner.work()[0].0;
        let mut visited = std::collections::HashSet::new();
        for _ in 0..count * 2 {
            let (index, _) = scanner.work()[0];
            visited.insert(index);
            scanner.complete(
                index,
                if index == hot {
                    vec![line(1800.0, 2000.0, "你好世界")]
                } else {
                    vec![]
                },
            );
            scanner.regions[hot].dirty = true;
            scanner.prepare(&dense, &token).unwrap();
        }
        assert_eq!(visited.len(), count);
        scanner.reset();
        assert!(scanner.previous.is_none());
        assert!(!scanner.pending());
        assert!(scanner.observations().is_empty());
    }
}
