//! Animated GIF and animated WebP export with integer nearest-neighbour upscaling.
//!
//! GIF frames share one exact global colour table built from the pixels (no quantisation), so
//! palette-locked input stays palette-locked. WebP frames are lossless VP8L bitstreams from the
//! pure-Rust `image-webp` encoder, wrapped in the animated WebP container (`VP8X`/`ANIM`/`ANMF`)
//! by this module.
use image::{codecs::webp::WebPEncoder, ExtendedColorType, RgbaImage};
use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct AnimFrame {
    pub image: RgbaImage,
    pub duration_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimOptions {
    /// Integer nearest-neighbour upscale factor (1 = unchanged).
    pub scale: u32,
    /// Total number of plays; 0 loops forever.
    pub loops: u16,
}

impl Default for AnimOptions {
    fn default() -> Self {
        AnimOptions { scale: 1, loops: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Gif,
    Webp,
}

impl Format {
    pub fn from_path(p: &Path) -> Option<Self> {
        match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "gif" => Some(Format::Gif),
            "webp" => Some(Format::Webp),
            _ => None,
        }
    }
}

/// Largest canvas side either format can hold (VP8L's limit; GIF allows 65535).
const MAX_WEBP_SIDE: u32 = 16384;
const MAX_WEBP_DURATION: u32 = (1 << 24) - 1;

/// Integer nearest-neighbour upscale: every source pixel becomes a `k`×`k` block.
pub fn scale_nearest(img: &RgbaImage, k: u32) -> RgbaImage {
    if k <= 1 {
        return img.clone();
    }
    RgbaImage::from_fn(img.width() * k, img.height() * k, |x, y| {
        *img.get_pixel(x / k, y / k)
    })
}

/// GIF frame delay in centiseconds. Rounded to the nearest 10 ms and clamped to at least 20 ms,
/// because browsers replace delays below 20 ms with 100 ms.
pub fn gif_delay_cs(ms: u32) -> u16 {
    (ms.saturating_add(5) / 10).clamp(2, u16::MAX as u32) as u16
}

fn prepare(
    frames: &[AnimFrame],
    o: &AnimOptions,
    max_side: u32,
) -> Result<(u32, u32, Vec<RgbaImage>), String> {
    let first = frames.first().ok_or("no frames")?;
    let (w, h) = first.image.dimensions();
    if w == 0 || h == 0 {
        return Err("frames must not be empty".into());
    }
    if let Some(f) = frames.iter().find(|f| f.image.dimensions() != (w, h)) {
        return Err(format!(
            "all frames must be the same size ({w}x{h} vs {}x{})",
            f.image.width(),
            f.image.height()
        ));
    }
    if o.scale == 0 {
        return Err("scale must be at least 1".into());
    }
    let (sw, sh) = (w as u64 * o.scale as u64, h as u64 * o.scale as u64);
    if sw > max_side as u64 || sh > max_side as u64 {
        return Err(format!(
            "{sw}x{sh} exceeds the format's {max_side} px limit"
        ));
    }
    Ok((
        sw as u32,
        sh as u32,
        frames
            .iter()
            .map(|f| scale_nearest(&f.image, o.scale))
            .collect(),
    ))
}

/// Encode an animated GIF. Alpha below 128 becomes transparent, anything else opaque.
/// Fails if the frames use more than 255 distinct opaque colours (quantize first).
pub fn encode_gif(frames: &[AnimFrame], o: &AnimOptions) -> Result<Vec<u8>, String> {
    let (w, h, imgs) = prepare(frames, o, u16::MAX as u32)?;
    let mut colors: Vec<[u8; 3]> = Vec::new();
    let mut index = std::collections::HashMap::new();
    let mut has_clear = false;
    for p in frames.iter().flat_map(|f| f.image.pixels()) {
        if p[3] < 128 {
            has_clear = true;
        } else if colors.len() <= 256 && !index.contains_key(&[p[0], p[1], p[2]]) {
            index.insert([p[0], p[1], p[2]], colors.len() as u8);
            colors.push([p[0], p[1], p[2]]);
        }
    }
    if colors.len() + has_clear as usize > 256 {
        return Err(
            "GIF allows at most 256 colours (255 with transparency); quantize first".into(),
        );
    }
    let clear = has_clear.then_some(colors.len() as u8);
    let mut table: Vec<u8> = colors.iter().flatten().copied().collect();
    if has_clear || table.is_empty() {
        table.extend([0, 0, 0]);
    }

    let mut out = Vec::new();
    {
        let mut enc = gif::Encoder::new(&mut out, w as u16, h as u16, &table)
            .map_err(|e| format!("gif: {e}"))?;
        match o.loops {
            0 => enc.set_repeat(gif::Repeat::Infinite),
            1 => Ok(()),
            n => enc.set_repeat(gif::Repeat::Finite(n - 1)),
        }
        .map_err(|e| format!("gif: {e}"))?;
        for (img, f) in imgs.iter().zip(frames) {
            let px: Vec<u8> = img
                .pixels()
                .map(|p| {
                    if p[3] < 128 {
                        clear.unwrap_or(0)
                    } else {
                        index[&[p[0], p[1], p[2]]]
                    }
                })
                .collect();
            let mut frame = gif::Frame::from_indexed_pixels(w as u16, h as u16, px, clear);
            frame.delay = gif_delay_cs(f.duration_ms);
            // Full-canvas frames: clear before the next one so transparent areas don't show stale pixels.
            frame.dispose = gif::DisposalMethod::Background;
            enc.write_frame(&frame).map_err(|e| format!("gif: {e}"))?;
        }
    }
    Ok(out)
}

fn chunk(fourcc: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut c = Vec::with_capacity(payload.len() + 9);
    c.extend_from_slice(fourcc);
    c.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    c.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        c.push(0);
    }
    c
}

fn u24(v: u32) -> [u8; 3] {
    let b = v.to_le_bytes();
    [b[0], b[1], b[2]]
}

/// Lossless still WebP for one frame, returned as its complete `VP8L` chunk (header + padding).
fn vp8l_chunk(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut still = Vec::new();
    WebPEncoder::new_lossless(&mut still)
        .encode(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|e| format!("webp: {e}"))?;
    if still.len() < 12 || &still[0..4] != b"RIFF" || &still[8..12] != b"WEBP" {
        return Err("webp: encoder produced an unexpected container".into());
    }
    let mut pos = 12;
    while pos + 8 <= still.len() {
        let size = u32::from_le_bytes(still[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let end = (pos + 8 + size + size % 2).min(still.len());
        if &still[pos..pos + 4] == b"VP8L" {
            return Ok(still[pos..end].to_vec());
        }
        pos = end;
    }
    Err("webp: encoder output has no VP8L chunk".into())
}

/// Encode a lossless animated WebP. Alpha is kept as-is; frames replace the canvas (no blending).
pub fn encode_webp(frames: &[AnimFrame], o: &AnimOptions) -> Result<Vec<u8>, String> {
    let (w, h, imgs) = prepare(frames, o, MAX_WEBP_SIDE)?;
    let mut vp8x = vec![0x10 | 0x02, 0, 0, 0]; // alpha + animation flags, 3 reserved bytes
    vp8x.extend(u24(w - 1));
    vp8x.extend(u24(h - 1));
    let mut anim = vec![0u8; 4]; // background BGRA: transparent
    anim.extend(o.loops.to_le_bytes());
    let mut body = b"WEBP".to_vec();
    body.extend(chunk(b"VP8X", &vp8x));
    body.extend(chunk(b"ANIM", &anim));
    for (img, f) in imgs.iter().zip(frames) {
        if f.duration_ms > MAX_WEBP_DURATION {
            return Err(format!(
                "frame duration {} ms exceeds WebP's {MAX_WEBP_DURATION} ms",
                f.duration_ms
            ));
        }
        let mut anmf = Vec::new();
        anmf.extend(u24(0)); // x / 2
        anmf.extend(u24(0)); // y / 2
        anmf.extend(u24(w - 1));
        anmf.extend(u24(h - 1));
        anmf.extend(u24(f.duration_ms));
        anmf.push(0b10); // do not blend, do not dispose
        anmf.extend(vp8l_chunk(img)?);
        body.extend(chunk(b"ANMF", &anmf));
    }
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    Ok(out)
}

pub fn encode(frames: &[AnimFrame], o: &AnimOptions, format: Format) -> Result<Vec<u8>, String> {
    match format {
        Format::Gif => encode_gif(frames, o),
        Format::Webp => encode_webp(frames, o),
    }
}
