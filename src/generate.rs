//! Procedural sprites: mask template + seeded noise + mirror + outline + shading.
use crate::palette::Palette;
use image::{Rgba, RgbaImage};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    Creature,
    Item,
    Tile,
}

/// Left halves (8 columns × 16 rows), mirrored to the right.
/// `.` always empty, `?` body 50%, `#` always body, `*` accent 60% (else body).
const CREATURE: [&str; 16] = [
    "........", "........", ".....??*", "....??##", "....?#*#", "...??###", "..??####", "..?*####",
    ".??#####", ".??##*##", "..??####", "...??###", "...??.##", "..??..?#", "..?...?.", "........",
];
const ITEM: [&str; 16] = [
    "........", "......??", "......#*", ".....?##", ".....?#*", "....??##", "....?###", "...??##*",
    "...?####", "...?##*#", "...?####", "....?###", ".....?##", "......#*", "......??", "........",
];

#[derive(Clone, Copy, PartialEq)]
enum Cell {
    Empty,
    Body,
    Accent,
}

pub fn validate_size(size: u32) -> Result<(), String> {
    if !(8..=64).contains(&size) || !size.is_multiple_of(2) {
        return Err(format!("size {size} must be even and in 8..=64"));
    }
    Ok(())
}

struct Ramp {
    outline: usize,
    shade: usize,
    base: usize,
    light: usize,
    accent: usize,
}

fn nearest_excluding(pal: &Palette, target: [f32; 3], skip: &[usize]) -> Option<usize> {
    (0..pal.len()).filter(|i| !skip.contains(i)).min_by(|a, b| {
        let d = |i: usize| {
            let p = pal.lab(i);
            (p[0] - target[0]).powi(2) + (p[1] - target[1]).powi(2) + (p[2] - target[2]).powi(2)
        };
        d(*a).total_cmp(&d(*b))
    })
}

fn pick_ramp(pal: &Palette, rng: &mut ChaCha8Rng) -> Ramp {
    let order = pal.by_lightness();
    let n = order.len();
    let outline = order[0];
    // Base from the middle band so there is room for a darker and a lighter step.
    let (lo, hi) = if n >= 4 { (2, n - 1) } else { (1, n) };
    let base = order[rng.gen_range(lo..hi)];
    let bl = pal.lab(base);
    let dark_t = [bl[0] - 0.15, bl[1] * 0.8, bl[2] * 0.8];
    let light_t = [bl[0] + 0.15, bl[1], bl[2]];
    let shade = nearest_excluding(pal, dark_t, &[base])
        .filter(|i| pal.lab(*i)[0] < bl[0])
        .unwrap_or(outline);
    let light = nearest_excluding(pal, light_t, &[base, outline])
        .filter(|i| pal.lab(*i)[0] > bl[0])
        .unwrap_or(base);
    let others: Vec<usize> = order[1..]
        .iter()
        .copied()
        .filter(|i| ![base, shade, light].contains(i))
        .collect();
    let accent = if others.is_empty() {
        light
    } else {
        others[rng.gen_range(0..others.len())]
    };
    Ramp {
        outline,
        shade,
        base,
        light,
        accent,
    }
}

/// Deterministic for a given (kind, size, seed, palette).
pub fn generate(kind: Kind, size: u32, seed: u64, pal: &Palette) -> Result<RgbaImage, String> {
    validate_size(size)?;
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ ((kind as u64) << 56));
    let ramp = pick_ramp(pal, &mut rng);
    let s = size as usize;
    let mut img = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 0]));

    if kind == Kind::Tile {
        for y in 0..size {
            for x in 0..size {
                let r: f32 = rng.gen();
                let c = if r < 0.12 {
                    ramp.shade
                } else if r > 0.92 {
                    ramp.light
                } else {
                    ramp.base
                };
                img.put_pixel(x, y, pal.rgba(c));
            }
        }
        return Ok(img);
    }

    let tpl = if kind == Kind::Creature {
        &CREATURE
    } else {
        &ITEM
    };
    // Noise on a coarse grid so large sprites get chunky shapes, not speckle.
    let g = s.min(24);
    let coarse: Vec<f32> = (0..g * g).map(|_| rng.gen()).collect();
    let half = s / 2;
    let mut grid = vec![Cell::Empty; s * s];
    for y in 0..s {
        for x in 0..half {
            let t = tpl[y * 16 / s].as_bytes()[x * 16 / s];
            let r = coarse[(y * g / s) * g + x * g / s];
            let c = match t {
                b'#' => Cell::Body,
                b'?' if r < 0.5 => Cell::Body,
                b'*' if r < 0.6 => Cell::Accent,
                b'*' => Cell::Body,
                _ => Cell::Empty,
            };
            grid[y * s + x] = c;
            grid[y * s + (s - 1 - x)] = c;
        }
    }
    let at = |x: i64, y: i64| -> Cell {
        if x < 0 || y < 0 || x >= s as i64 || y >= s as i64 {
            Cell::Empty
        } else {
            grid[y as usize * s + x as usize]
        }
    };
    for y in 0..s as i64 {
        for x in 0..s as i64 {
            let c = at(x, y);
            let col = if c == Cell::Empty {
                let near = [(0, -1), (0, 1), (-1, 0), (1, 0)]
                    .iter()
                    .any(|(dx, dy)| at(x + dx, y + dy) != Cell::Empty);
                if !near {
                    continue;
                }
                ramp.outline
            } else if c == Cell::Accent {
                ramp.accent
            } else if at(x, y - 1) == Cell::Empty {
                ramp.light
            } else if at(x, y + 1) == Cell::Empty || at(x, y + 2) == Cell::Empty {
                ramp.shade
            } else {
                ramp.base
            };
            img.put_pixel(x as u32, y as u32, pal.rgba(col));
        }
    }
    Ok(img)
}
