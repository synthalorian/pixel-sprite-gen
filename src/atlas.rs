//! Frame atlas (TexturePacker "JSON array" layout plus per-frame `duration` in milliseconds).
//!
//! Frame order is significant: it is the playback order used by [`Atlas::anim_frames`].
use crate::anim::AnimFrame;
use image::{imageops, RgbaImage};
use serde::{Deserialize, Serialize};

/// One duration for all `n` frames, or exactly one per frame.
pub fn expand_durations(ms: &[u32], n: usize) -> Result<Vec<u32>, String> {
    match ms.len() {
        1 => Ok(vec![ms[0]; n]),
        m if m == n => Ok(ms.to_vec()),
        m => Err(format!("{m} durations given for {n} frames (use 1 or {n})")),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> u32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> u32 {
        self.y + self.h
    }
    pub fn area(&self) -> u64 {
        self.w as u64 * self.h as u64
    }
    /// Smallest rectangle containing both.
    pub fn union(&self, o: &Rect) -> Rect {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Rect::new(
            x,
            y,
            self.right().max(o.right()) - x,
            self.bottom().max(o.bottom()) - y,
        )
    }
    /// True if the rectangles share a pixel, or (for `gap` > 0) are separated by at most `gap`
    /// empty pixels on each axis.
    pub fn near(&self, o: &Rect, gap: u32) -> bool {
        let g = if gap > 0 { gap + 1 } else { 0 };
        self.x < o.right() + g
            && o.x < self.right() + g
            && self.y < o.bottom() + g
            && o.y < self.bottom() + g
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

/// Normalised pivot relative to the frame's source (untrimmed) size: (0,0) top-left, (1,1) bottom-right.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pivot {
    pub x: f32,
    pub y: f32,
}

impl Default for Pivot {
    fn default() -> Self {
        Pivot { x: 0.5, y: 0.5 }
    }
}

impl std::str::FromStr for Pivot {
    type Err = String;
    /// `X,Y` with both in 0..=1, e.g. `0.5,1`.
    fn from_str(s: &str) -> Result<Self, String> {
        let (x, y) = s
            .split_once(',')
            .ok_or(format!("pivot {s:?}: expected X,Y"))?;
        let p = |v: &str| -> Result<f32, String> {
            let f: f32 = v
                .trim()
                .parse()
                .map_err(|_| format!("pivot {s:?}: bad number {v:?}"))?;
            if !(0.0..=1.0).contains(&f) {
                return Err(format!("pivot {s:?}: components must be in 0..=1"));
            }
            Ok(f)
        };
        Ok(Pivot { x: p(x)?, y: p(y)? })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtlasFrame {
    pub name: String,
    /// Region of the sheet holding the (possibly trimmed) pixels.
    pub frame: Rect,
    pub trimmed: bool,
    /// Where `frame` sits inside the untrimmed source rectangle.
    pub sprite_source_size: Rect,
    pub source_size: Size,
    pub pivot: Pivot,
    /// Display time in milliseconds.
    pub duration: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub app: String,
    pub version: String,
    pub image: String,
    pub size: Size,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Atlas {
    pub frames: Vec<AtlasFrame>,
    pub meta: Meta,
}

impl Atlas {
    pub fn new(image: impl Into<String>, size: (u32, u32), frames: Vec<AtlasFrame>) -> Self {
        Atlas {
            frames,
            meta: Meta {
                app: env!("CARGO_PKG_NAME").into(),
                version: env!("CARGO_PKG_VERSION").into(),
                image: image.into(),
                size: Size {
                    w: size.0,
                    h: size.1,
                },
            },
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("atlas serialises") + "\n"
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let a: Atlas = serde_json::from_str(text).map_err(|e| format!("atlas: {e}"))?;
        for f in &a.frames {
            let s = &f.sprite_source_size;
            if f.frame.w != s.w
                || f.frame.h != s.h
                || s.right() > f.source_size.w
                || s.bottom() > f.source_size.h
            {
                return Err(format!("atlas frame {:?}: inconsistent sizes", f.name));
            }
        }
        Ok(a)
    }

    /// Replaces every frame's duration (see [`expand_durations`]); an empty list keeps them.
    pub fn set_durations(&mut self, ms: &[u32]) -> Result<(), String> {
        if !ms.is_empty() {
            let d = expand_durations(ms, self.frames.len())?;
            self.frames
                .iter_mut()
                .zip(d)
                .for_each(|(f, d)| f.duration = d);
        }
        Ok(())
    }

    /// The frame's pixels restored to its untrimmed source size.
    pub fn extract(&self, sheet: &RgbaImage, i: usize) -> Result<RgbaImage, String> {
        let f = self.frames.get(i).ok_or(format!("no frame {i}"))?;
        let r = f.frame;
        if r.right() > sheet.width() || r.bottom() > sheet.height() {
            return Err(format!(
                "frame {:?} lies outside the {}x{} sheet",
                f.name,
                sheet.width(),
                sheet.height()
            ));
        }
        let px = imageops::crop_imm(sheet, r.x, r.y, r.w, r.h).to_image();
        if !f.trimmed && (r.w, r.h) == (f.source_size.w, f.source_size.h) {
            return Ok(px);
        }
        let mut out = RgbaImage::new(f.source_size.w, f.source_size.h);
        let s = f.sprite_source_size;
        imageops::replace(&mut out, &px, s.x as i64, s.y as i64);
        Ok(out)
    }

    /// All frames on one shared canvas with their pivots aligned, ready for [`crate::anim`].
    /// Frames with equal source sizes and pivots are passed through unchanged.
    pub fn anim_frames(&self, sheet: &RgbaImage) -> Result<Vec<AnimFrame>, String> {
        if self.frames.is_empty() {
            return Err("atlas has no frames".into());
        }
        let anchor = |f: &AtlasFrame| {
            (
                (f.pivot.x * f.source_size.w as f32).round() as i64,
                (f.pivot.y * f.source_size.h as f32).round() as i64,
            )
        };
        let (mut left, mut top, mut right, mut bottom) = (0i64, 0i64, 0i64, 0i64);
        for f in &self.frames {
            let (ax, ay) = anchor(f);
            left = left.max(ax);
            top = top.max(ay);
            right = right.max(f.source_size.w as i64 - ax);
            bottom = bottom.max(f.source_size.h as i64 - ay);
        }
        let (cw, ch) = ((left + right) as u32, (top + bottom) as u32);
        (0..self.frames.len())
            .map(|i| {
                let f = &self.frames[i];
                let src = self.extract(sheet, i)?;
                let (ax, ay) = anchor(f);
                let image = if src.dimensions() == (cw, ch) {
                    src
                } else {
                    let mut c = RgbaImage::new(cw, ch);
                    imageops::replace(&mut c, &src, left - ax, top - ay);
                    c
                };
                Ok(AnimFrame {
                    image,
                    duration_ms: f.duration,
                })
            })
            .collect()
    }
}
