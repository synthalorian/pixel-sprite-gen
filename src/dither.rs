use crate::palette::{to_oklab, Palette};
use image::{Rgba, RgbaImage};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Dither {
    None,
    Bayer,
    Floyd,
}

const BAYER4: [[f32; 4]; 4] = [
    [0.0, 8.0, 2.0, 10.0],
    [12.0, 4.0, 14.0, 6.0],
    [3.0, 11.0, 1.0, 9.0],
    [15.0, 7.0, 13.0, 5.0],
];

/// Map an image onto the palette. Alpha < 128 becomes transparent, everything else opaque.
/// Error is computed in OKLab so dithering matches the nearest-colour metric.
pub fn quantize(img: &RgbaImage, pal: &Palette, mode: Dither) -> RgbaImage {
    let (w, h) = img.dimensions();
    let mut lab: Vec<[f32; 3]> = img.pixels().map(|p| to_oklab([p[0], p[1], p[2]])).collect();
    let mut out = RgbaImage::new(w, h);
    // Bayer spread: a fraction of the palette's typical lightness gap.
    let spread = {
        let o = pal.by_lightness();
        let span = pal.lab(o[o.len() - 1])[0] - pal.lab(o[0])[0];
        span / (pal.len() as f32 - 1.0).max(1.0)
    };
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if img.get_pixel(x, y)[3] < 128 {
                out.put_pixel(x, y, Rgba([0, 0, 0, 0]));
                continue;
            }
            let mut c = lab[i];
            if mode == Dither::Bayer {
                let t = (BAYER4[(y % 4) as usize][(x % 4) as usize] + 0.5) / 16.0 - 0.5;
                c[0] += t * spread;
            }
            let k = pal.nearest_lab(c);
            out.put_pixel(x, y, pal.rgba(k));
            if mode == Dither::Floyd {
                let p = pal.lab(k);
                let e = [c[0] - p[0], c[1] - p[1], c[2] - p[2]];
                let mut push = |dx: i64, dy: i64, f: f32| {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx >= 0 && nx < w as i64 && ny < h as i64 {
                        let j = (ny as u32 * w + nx as u32) as usize;
                        for (ch, ev) in e.iter().enumerate() {
                            lab[j][ch] += ev * f;
                        }
                    }
                };
                push(1, 0, 7.0 / 16.0);
                push(-1, 1, 3.0 / 16.0);
                push(0, 1, 5.0 / 16.0);
                push(1, 1, 1.0 / 16.0);
            }
        }
    }
    out
}
