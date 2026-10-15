use clap::{Parser, Subcommand};
use spritegen::{
    dither::{quantize, Dither},
    generate::{generate, Kind},
    sheet, Palette,
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "sprite",
    version,
    about = "Palette-locked pixel-art sprite generator"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate one procedural sprite
    Gen {
        #[arg(long, value_enum, default_value = "creature")]
        kind: Kind,
        #[arg(long, default_value_t = 16)]
        size: u32,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// blackshield | gb4 | pico8 | path to .hex/.gpl
        #[arg(long, default_value = "blackshield")]
        palette: String,
        /// Integer upscale for previewing (nearest neighbour)
        #[arg(long, default_value_t = 1)]
        scale: u32,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Map an image onto a palette
    Quantize {
        input: PathBuf,
        #[arg(long, default_value = "blackshield")]
        palette: String,
        #[arg(long, value_enum, default_value = "bayer")]
        dither: Dither,
        /// Resize (nearest) so the longest side is this many pixels before quantizing
        #[arg(long)]
        max_size: Option<u32>,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// N variants by consecutive seeds packed into a sheet + JSON atlas
    Sheet {
        #[arg(long, value_enum, default_value = "creature")]
        kind: Kind,
        #[arg(long, default_value_t = 16)]
        size: u32,
        #[arg(long, default_value_t = 1)]
        seed: u64,
        #[arg(long, default_value_t = 16)]
        count: u32,
        #[arg(long, default_value_t = 8)]
        cols: u32,
        #[arg(long, default_value_t = 0)]
        pad: u32,
        #[arg(long, default_value = "blackshield")]
        palette: String,
        /// Output PNG; the atlas is written next to it with a .json extension
        #[arg(short, long)]
        out: PathBuf,
    },
    /// List built-in palettes or print a palette's colours
    Palette { name: Option<String> },
}

fn save(img: &image::RgbaImage, pal: &Palette, out: &PathBuf) -> Result<(), String> {
    pal.check_image(img)?;
    img.save(out).map_err(|e| format!("{}: {e}", out.display()))
}

fn run() -> Result<(), String> {
    match Cli::parse().cmd {
        Cmd::Gen {
            kind,
            size,
            seed,
            palette,
            scale,
            out,
        } => {
            let pal = Palette::load(&palette)?;
            let mut img = generate(kind, size, seed, &pal)?;
            if scale > 1 {
                img = image::imageops::resize(
                    &img,
                    size * scale,
                    size * scale,
                    image::imageops::FilterType::Nearest,
                );
            }
            save(&img, &pal, &out)
        }
        Cmd::Quantize {
            input,
            palette,
            dither,
            max_size,
            out,
        } => {
            let pal = Palette::load(&palette)?;
            let mut img = image::open(&input)
                .map_err(|e| format!("{}: {e}", input.display()))?
                .to_rgba8();
            if let Some(m) = max_size {
                let (w, h) = img.dimensions();
                let k = m as f32 / w.max(h) as f32;
                if k < 1.0 {
                    let (nw, nh) = (
                        ((w as f32 * k).round() as u32).max(1),
                        ((h as f32 * k).round() as u32).max(1),
                    );
                    img = image::imageops::resize(
                        &img,
                        nw,
                        nh,
                        image::imageops::FilterType::Triangle,
                    );
                }
            }
            save(&quantize(&img, &pal, dither), &pal, &out)
        }
        Cmd::Sheet {
            kind,
            size,
            seed,
            count,
            cols,
            pad,
            palette,
            out,
        } => {
            let pal = Palette::load(&palette)?;
            let frames = (0..count.max(1) as u64)
                .map(|i| {
                    Ok(sheet::Frame {
                        name: format!("{kind:?}_{}", seed + i).to_lowercase(),
                        image: generate(kind, size, seed + i, &pal)?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let name = out
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("sheet.png")
                .to_string();
            let (img, json) = sheet::pack(&frames, cols, pad, &name)?;
            save(&img, &pal, &out)?;
            let jp = out.with_extension("json");
            std::fs::write(&jp, json).map_err(|e| format!("{}: {e}", jp.display()))
        }
        Cmd::Palette { name } => {
            match name {
                None => spritegen::palette::BUILTINS
                    .iter()
                    .for_each(|b| println!("{b}")),
                Some(n) => Palette::load(&n)?
                    .colors
                    .iter()
                    .for_each(|c| println!("{:02x}{:02x}{:02x}", c[0], c[1], c[2])),
            }
            Ok(())
        }
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
