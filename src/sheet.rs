//! Sprite sheet + JSON atlas (TexturePacker-style "frames" hash).
use image::{imageops, RgbaImage};

pub struct Frame {
    pub name: String,
    pub image: RgbaImage,
}

/// Packs equal-sized frames into a grid `cols` wide with `pad` transparent pixels between cells.
pub fn pack(
    frames: &[Frame],
    cols: u32,
    pad: u32,
    image_name: &str,
) -> Result<(RgbaImage, String), String> {
    let first = frames.first().ok_or("no frames")?;
    let (fw, fh) = first.image.dimensions();
    if frames.iter().any(|f| f.image.dimensions() != (fw, fh)) {
        return Err("all frames must be the same size".into());
    }
    let cols = cols.max(1).min(frames.len() as u32);
    let rows = (frames.len() as u32).div_ceil(cols);
    let (w, h) = (cols * fw + (cols - 1) * pad, rows * fh + (rows - 1) * pad);
    let mut sheet = RgbaImage::new(w, h);
    let mut json = String::from("{\n  \"frames\": {\n");
    for (i, f) in frames.iter().enumerate() {
        let (cx, cy) = (
            (i as u32 % cols) * (fw + pad),
            (i as u32 / cols) * (fh + pad),
        );
        imageops::replace(&mut sheet, &f.image, cx as i64, cy as i64);
        json.push_str(&format!(
            "    \"{}\": {{ \"frame\": {{ \"x\": {cx}, \"y\": {cy}, \"w\": {fw}, \"h\": {fh} }} }}{}\n",
            esc(&f.name),
            if i + 1 < frames.len() { "," } else { "" }
        ));
    }
    json.push_str(&format!(
        "  }},\n  \"meta\": {{ \"image\": \"{}\", \"size\": {{ \"w\": {w}, \"h\": {h} }}, \"frame_size\": {{ \"w\": {fw}, \"h\": {fh} }}, \"count\": {} }}\n}}\n",
        esc(image_name),
        frames.len()
    ));
    Ok((sheet, json))
}

fn esc(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            '"' => vec!['\\', '"'],
            '\\' => vec!['\\', '\\'],
            c if (c as u32) < 0x20 => format!("\\u{:04x}", c as u32).chars().collect(),
            c => vec![c],
        })
        .collect()
}
