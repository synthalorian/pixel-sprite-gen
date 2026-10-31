//! Sprite-sheet slicing: fixed grid (optionally trimmed per cell) or connected alpha regions.
use crate::atlas::{Atlas, AtlasFrame, Pivot, Rect, Size};
use image::RgbaImage;
use std::collections::VecDeque;

/// Tiled-style grid: `margin` px around the sheet edge, `spacing` px between cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Grid {
    pub cell_w: u32,
    pub cell_h: u32,
    pub margin: u32,
    pub spacing: u32,
}

/// Options shared by both slicing modes.
#[derive(Clone, Debug)]
pub struct SliceOptions {
    /// Frame names are `{prefix}_{index}`.
    pub prefix: String,
    pub pivot: Pivot,
    /// One duration for every frame, or exactly one per emitted frame.
    pub durations: Vec<u32>,
    /// A pixel is solid when its alpha is above this.
    pub alpha_threshold: u8,
}

impl Default for SliceOptions {
    fn default() -> Self {
        SliceOptions {
            prefix: "frame".into(),
            pivot: Pivot::default(),
            durations: vec![100],
            alpha_threshold: 0,
        }
    }
}

/// Every whole cell of the grid in reading order. Partial cells at the right/bottom edge are ignored.
pub fn grid_cells(width: u32, height: u32, g: &Grid) -> Result<Vec<Rect>, String> {
    if g.cell_w == 0 || g.cell_h == 0 {
        return Err("cell size must be non-zero".into());
    }
    let fit = |total: u32, cell: u32| -> u32 {
        let avail = total.saturating_sub(2 * g.margin);
        if avail < cell {
            0
        } else {
            (avail - cell) / (cell + g.spacing) + 1
        }
    };
    let (cols, rows) = (fit(width, g.cell_w), fit(height, g.cell_h));
    if cols == 0 || rows == 0 {
        return Err(format!(
            "no {}x{} cells fit in a {width}x{height} image with margin {}",
            g.cell_w, g.cell_h, g.margin
        ));
    }
    Ok((0..rows)
        .flat_map(|r| {
            (0..cols).map(move |c| {
                Rect::new(
                    g.margin + c * (g.cell_w + g.spacing),
                    g.margin + r * (g.cell_h + g.spacing),
                    g.cell_w,
                    g.cell_h,
                )
            })
        })
        .collect())
}

/// Tight bounds of the solid pixels inside `area`, or `None` if it is empty.
pub fn alpha_bounds(img: &RgbaImage, area: Rect, threshold: u8) -> Option<Rect> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if img.get_pixel(x, y)[3] > threshold {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    (x0 != u32::MAX).then(|| Rect::new(x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

/// Bounding boxes of 8-connected solid regions. Boxes that overlap, or come within `merge_gap`
/// pixels of each other, are merged so detached parts of one sprite stay together. Boxes with
/// fewer than `min_pixels` solid pixels are dropped. Result is in reading order (rows top to
/// bottom, a new row starting when a box begins below every box in the current row).
pub fn connected_regions(
    img: &RgbaImage,
    threshold: u8,
    merge_gap: u32,
    min_pixels: u32,
) -> Vec<Rect> {
    let (w, h) = img.dimensions();
    let solid = |x: u32, y: u32| img.get_pixel(x, y)[3] > threshold;
    let mut seen = vec![false; (w * h) as usize];
    let mut regions: Vec<(Rect, u32)> = Vec::new();
    let mut queue = VecDeque::new();
    for sy in 0..h {
        for sx in 0..w {
            if seen[(sy * w + sx) as usize] || !solid(sx, sy) {
                continue;
            }
            seen[(sy * w + sx) as usize] = true;
            queue.push_back((sx, sy));
            let (mut r, mut n) = (Rect::new(sx, sy, 1, 1), 0u32);
            while let Some((x, y)) = queue.pop_front() {
                n += 1;
                r = r.union(&Rect::new(x, y, 1, 1));
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                        if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                            continue;
                        }
                        let (nx, ny) = (nx as u32, ny as u32);
                        let k = (ny * w + nx) as usize;
                        if !seen[k] && solid(nx, ny) {
                            seen[k] = true;
                            queue.push_back((nx, ny));
                        }
                    }
                }
            }
            regions.push((r, n));
        }
    }
    // Merge until stable: a merged box can grow into a neighbour it did not touch before.
    let mut merged = true;
    while merged {
        merged = false;
        let mut i = 0;
        while i < regions.len() {
            let mut j = i + 1;
            while j < regions.len() {
                if regions[i].0.near(&regions[j].0, merge_gap) {
                    let (rj, nj) = regions.swap_remove(j);
                    regions[i] = (regions[i].0.union(&rj), regions[i].1 + nj);
                    merged = true;
                    j = i + 1; // the grown box may now reach earlier candidates
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
    let mut rects: Vec<Rect> = regions
        .into_iter()
        .filter(|(_, n)| *n >= min_pixels)
        .map(|(r, _)| r)
        .collect();
    rects.sort_by_key(|r| (r.y, r.x));
    let mut rows: Vec<Vec<Rect>> = Vec::new();
    let mut row_bottom = 0;
    for r in rects {
        match rows.last_mut() {
            Some(row) if r.y < row_bottom => {
                row_bottom = row_bottom.max(r.bottom());
                row.push(r);
            }
            _ => {
                row_bottom = r.bottom();
                rows.push(vec![r]);
            }
        }
    }
    rows.into_iter()
        .flat_map(|mut row| {
            row.sort_by_key(|r| (r.x, r.y));
            row
        })
        .collect()
}

fn frame(name: String, frame: Rect, source: Rect, pivot: Pivot) -> AtlasFrame {
    AtlasFrame {
        name,
        trimmed: frame != source,
        sprite_source_size: Rect::new(frame.x - source.x, frame.y - source.y, frame.w, frame.h),
        source_size: Size {
            w: source.w,
            h: source.h,
        },
        frame,
        pivot,
        duration: 0,
    }
}

fn finish(
    img: &RgbaImage,
    image_name: &str,
    frames: Vec<AtlasFrame>,
    o: &SliceOptions,
) -> Result<Atlas, String> {
    if frames.is_empty() {
        return Err("no non-empty frames found".into());
    }
    let mut atlas = Atlas::new(image_name, img.dimensions(), frames);
    atlas.set_durations(if o.durations.is_empty() {
        &[100]
    } else {
        &o.durations
    })?;
    Ok(atlas)
}

/// Slice by grid. With `trim`, each frame is cropped to its cell's alpha bounds and records its
/// offset in the cell (`spriteSourceSize`), so the pivot still refers to the full cell.
/// Fully transparent cells are skipped unless `keep_empty`.
pub fn slice_grid(
    img: &RgbaImage,
    image_name: &str,
    grid: &Grid,
    trim: bool,
    keep_empty: bool,
    o: &SliceOptions,
) -> Result<Atlas, String> {
    let mut frames = Vec::new();
    for cell in grid_cells(img.width(), img.height(), grid)? {
        let bounds = alpha_bounds(img, cell, o.alpha_threshold);
        let r = match (bounds, trim) {
            (None, _) if !keep_empty => continue,
            (None, _) => cell,
            (Some(b), true) => b,
            (Some(_), false) => cell,
        };
        frames.push(frame(
            format!("{}_{}", o.prefix, frames.len()),
            r,
            cell,
            o.pivot,
        ));
    }
    finish(img, image_name, frames, o)
}

/// Slice by connected alpha regions (see [`connected_regions`]). Each frame is its region's
/// bounding box, untrimmed, with the pivot relative to that box.
pub fn slice_alpha(
    img: &RgbaImage,
    image_name: &str,
    merge_gap: u32,
    min_pixels: u32,
    o: &SliceOptions,
) -> Result<Atlas, String> {
    let frames = connected_regions(img, o.alpha_threshold, merge_gap, min_pixels)
        .into_iter()
        .enumerate()
        .map(|(i, r)| frame(format!("{}_{i}", o.prefix), r, r, o.pivot))
        .collect();
    finish(img, image_name, frames, o)
}
