use image::{Rgba, RgbaImage};
use palette::{IntoColor, Oklab, Srgb};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<[u8; 3]>,
    lab: Vec<[f32; 3]>,
}

const BLACKSHIELD: &str = include_str!("../palettes/blackshield.hex");
const GB4: &str = "0f380f\n306230\n8bac0f\n9bbc0f\n";
const PICO8: &str = "000000\n1d2b53\n7e2553\n008751\nab5236\n5f574f\nc2c3c7\nfff1e8\nff004d\nffa300\nffec27\n00e436\n29adff\n83769c\nff77a8\nffccaa\n";

pub const BUILTINS: &[&str] = &["blackshield", "gb4", "pico8"];

pub fn to_oklab(c: [u8; 3]) -> [f32; 3] {
    let s: Srgb<f32> = Srgb::new(c[0], c[1], c[2]).into_format();
    let l: Oklab = s.into_linear().into_color();
    [l.l, l.a, l.b]
}

fn parse_hex_color(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    let s = if s.len() == 8 { &s[2..] } else { s }; // allow AARRGGBB
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

impl Palette {
    pub fn new(name: impl Into<String>, colors: Vec<[u8; 3]>) -> Result<Self, String> {
        let mut uniq: Vec<[u8; 3]> = Vec::new();
        for c in colors {
            if !uniq.contains(&c) {
                uniq.push(c);
            }
        }
        if uniq.len() < 2 {
            return Err("palette needs at least 2 distinct colours".into());
        }
        let lab = uniq.iter().map(|c| to_oklab(*c)).collect();
        Ok(Palette {
            name: name.into(),
            colors: uniq,
            lab,
        })
    }

    /// One colour per line, `RRGGBB` or `#RRGGBB` (Lospec .hex). Blank and `;`/`//` lines ignored.
    pub fn parse_hex(name: &str, text: &str) -> Result<Self, String> {
        let mut cols = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let t = line.trim();
            if t.is_empty() || t.starts_with(';') || t.starts_with("//") {
                continue;
            }
            cols.push(parse_hex_color(t).ok_or(format!("line {}: bad colour {t:?}", i + 1))?);
        }
        Self::new(name, cols)
    }

    /// GIMP palette (.gpl).
    pub fn parse_gpl(name: &str, text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some("GIMP Palette") {
            return Err("missing 'GIMP Palette' header".into());
        }
        let mut name = name.to_string();
        let mut cols = Vec::new();
        for line in lines {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            if let Some(n) = t.strip_prefix("Name:") {
                name = n.trim().to_string();
                continue;
            }
            if t.starts_with("Columns:") {
                continue;
            }
            let nums: Vec<u8> = t
                .split_whitespace()
                .take(3)
                .map_while(|p| p.parse().ok())
                .collect();
            if nums.len() != 3 {
                return Err(format!("bad gpl line {t:?}"));
            }
            cols.push([nums[0], nums[1], nums[2]]);
        }
        Self::new(name, cols)
    }

    pub fn builtin(name: &str) -> Option<Self> {
        let src = match name {
            "blackshield" => BLACKSHIELD,
            "gb4" => GB4,
            "pico8" => PICO8,
            _ => return None,
        };
        Some(Self::parse_hex(name, src).expect("builtin palette valid"))
    }

    /// Built-in name or path to a .hex / .gpl file.
    pub fn load(spec: &str) -> Result<Self, String> {
        if let Some(p) = Self::builtin(spec) {
            return Ok(p);
        }
        let path = Path::new(spec);
        let text = std::fs::read_to_string(path).map_err(|e| format!("{spec}: {e}"))?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("custom");
        if text.trim_start().starts_with("GIMP Palette") {
            Self::parse_gpl(stem, &text)
        } else {
            Self::parse_hex(stem, &text)
        }
    }

    pub fn len(&self) -> usize {
        self.colors.len()
    }
    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }
    pub fn lab(&self, i: usize) -> [f32; 3] {
        self.lab[i]
    }

    /// Index of the nearest colour in OKLab (squared Euclidean). Ties go to the lowest index.
    pub fn nearest_lab(&self, l: [f32; 3]) -> usize {
        let mut best = 0;
        let mut bd = f32::MAX;
        for (i, p) in self.lab.iter().enumerate() {
            let d = (p[0] - l[0]).powi(2) + (p[1] - l[1]).powi(2) + (p[2] - l[2]).powi(2);
            if d < bd {
                bd = d;
                best = i;
            }
        }
        best
    }

    pub fn nearest(&self, c: [u8; 3]) -> usize {
        self.nearest_lab(to_oklab(c))
    }

    /// Palette indices sorted dark → light by OKLab L.
    pub fn by_lightness(&self) -> Vec<usize> {
        let mut idx: Vec<usize> = (0..self.len()).collect();
        idx.sort_by(|a, b| self.lab[*a][0].total_cmp(&self.lab[*b][0]).then(a.cmp(b)));
        idx
    }

    pub fn rgba(&self, i: usize) -> Rgba<u8> {
        let c = self.colors[i];
        Rgba([c[0], c[1], c[2], 255])
    }

    pub fn contains(&self, c: [u8; 3]) -> bool {
        self.colors.contains(&c)
    }

    /// Every pixel must be fully transparent or an exact opaque palette colour.
    pub fn check_image(&self, img: &RgbaImage) -> Result<(), String> {
        for (x, y, p) in img.enumerate_pixels() {
            let [r, g, b, a] = p.0;
            if a == 0 {
                continue;
            }
            if a != 255 || !self.contains([r, g, b]) {
                return Err(format!(
                    "pixel ({x},{y}) = {:?} not in palette '{}'",
                    p.0, self.name
                ));
            }
        }
        Ok(())
    }
}
