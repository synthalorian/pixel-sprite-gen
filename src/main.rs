use clap::{Args, Parser, Subcommand};
use spritegen::{
    anim::{self, AnimFrame, AnimOptions, Format},
    atlas::{expand_durations, Atlas, Pivot},
    dither::{quantize, Dither},
    generate::{generate, Kind},
    sheet,
    slice::{self, Grid, SliceOptions},
    Palette,
};
use std::path::{Path, PathBuf};

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
    /// Check images are palette-locked; lists off-palette pixels and exits non-zero on failure
    Verify {
        #[arg(required = true)]
        images: Vec<PathBuf>,
        #[arg(long, default_value = "blackshield")]
        palette: String,
        /// Maximum offending pixels listed per image (0 = all)
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Slice a sprite sheet into frames and write a JSON atlas
    Slice {
        #[command(subcommand)]
        mode: SliceMode,
    },
    /// Export an animated GIF or WebP from frame PNGs or an atlas
    Anim {
        /// Frame images in playback order (alternative to --atlas)
        #[arg(required_unless_present = "atlas", conflicts_with = "atlas")]
        frames: Vec<PathBuf>,
        /// Atlas JSON from `sprite slice`; frames play in atlas order, pivots aligned
        #[arg(long)]
        atlas: Option<PathBuf>,
        /// Sheet for --atlas (default: meta.image, relative to the atlas)
        #[arg(long, requires = "atlas")]
        image: Option<PathBuf>,
        /// Frame duration in ms: one value for all, or comma-separated per frame.
        /// Overrides atlas durations; frame PNGs default to 100.
        #[arg(long, value_delimiter = ',')]
        duration: Vec<u32>,
        /// Integer nearest-neighbour upscale
        #[arg(long, default_value_t = 1)]
        scale: u32,
        /// Number of plays; 0 loops forever
        #[arg(long, default_value_t = 0)]
        loops: u16,
        /// Output format (default: from the --out extension)
        #[arg(long, value_enum)]
        format: Option<Format>,
        /// Fail unless every frame is locked to this palette
        #[arg(long)]
        palette: Option<String>,
        #[arg(short, long)]
        out: PathBuf,
    },
}

#[derive(Subcommand)]
enum SliceMode {
    /// Fixed cells with optional margin/spacing; empty cells are skipped
    Grid {
        #[command(flatten)]
        common: SliceArgs,
        /// Cell size, WxH or N for square
        #[arg(long, value_parser = parse_cell)]
        cell: (u32, u32),
        /// Pixels between the sheet edge and the first cell
        #[arg(long, default_value_t = 0)]
        margin: u32,
        /// Pixels between cells
        #[arg(long, default_value_t = 0)]
        spacing: u32,
        /// Crop each frame to its alpha bounds (offset kept in spriteSourceSize)
        #[arg(long)]
        trim: bool,
        /// Keep fully transparent cells
        #[arg(long)]
        keep_empty: bool,
    },
    /// One frame per connected opaque region (bounding box)
    Alpha {
        #[command(flatten)]
        common: SliceArgs,
        /// Merge regions whose boxes come within this many pixels (0 = only overlapping boxes)
        #[arg(long, default_value_t = 0)]
        merge_gap: u32,
        /// Drop regions with fewer opaque pixels than this
        #[arg(long, default_value_t = 1)]
        min_pixels: u32,
    },
}

#[derive(Args)]
struct SliceArgs {
    input: PathBuf,
    /// Normalised pivot X,Y relative to the untrimmed frame (0,0 = top-left)
    #[arg(long, default_value = "0.5,0.5")]
    pivot: Pivot,
    /// Frame duration in ms: one value for all, or comma-separated per frame
    #[arg(long, value_delimiter = ',', default_value = "100")]
    duration: Vec<u32>,
    /// Pixels with alpha above this count as opaque
    #[arg(long, default_value_t = 0)]
    alpha_threshold: u8,
    /// Frame name prefix (default: input file stem)
    #[arg(long)]
    prefix: Option<String>,
    /// Also write each frame (untrimmed) as `<name>.png` into this directory
    #[arg(long)]
    frames_dir: Option<PathBuf>,
    /// Atlas JSON path
    #[arg(short, long)]
    out: PathBuf,
}

fn parse_cell(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s.split_once(['x', 'X']).unwrap_or((s, s));
    let n = |v: &str| {
        v.trim()
            .parse::<u32>()
            .map_err(|_| format!("bad cell size {s:?}"))
    };
    Ok((n(w)?, n(h)?))
}

fn load_rgba(p: &Path) -> Result<image::RgbaImage, String> {
    Ok(image::open(p)
        .map_err(|e| format!("{}: {e}", p.display()))?
        .to_rgba8())
}

/// `target` relative to directory `base` when both resolve, else `target` as given.
fn relative_to(target: &Path, base: &Path) -> String {
    let (Ok(t), Ok(b)) = (target.canonicalize(), base.canonicalize()) else {
        return target.display().to_string();
    };
    let (tc, bc): (Vec<_>, Vec<_>) = (t.components().collect(), b.components().collect());
    let common = tc.iter().zip(&bc).take_while(|(a, b)| a == b).count();
    let mut rel = PathBuf::new();
    (common..bc.len()).for_each(|_| rel.push(".."));
    tc[common..].iter().for_each(|c| rel.push(c));
    rel.to_string_lossy().replace('\\', "/")
}

fn slice_cmd(mode: SliceMode) -> Result<(), String> {
    let (SliceMode::Grid { common: c, .. } | SliceMode::Alpha { common: c, .. }) = &mode;
    let img = load_rgba(&c.input)?;
    let prefix = c.prefix.clone().unwrap_or_else(|| {
        c.input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("frame")
            .to_string()
    });
    let o = SliceOptions {
        prefix,
        pivot: c.pivot,
        durations: c.duration.clone(),
        alpha_threshold: c.alpha_threshold,
    };
    let out_dir = c
        .out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = relative_to(&c.input, out_dir);
    let atlas = match &mode {
        SliceMode::Grid {
            cell,
            margin,
            spacing,
            trim,
            keep_empty,
            ..
        } => {
            let g = Grid {
                cell_w: cell.0,
                cell_h: cell.1,
                margin: *margin,
                spacing: *spacing,
            };
            slice::slice_grid(&img, &name, &g, *trim, *keep_empty, &o)?
        }
        SliceMode::Alpha {
            merge_gap,
            min_pixels,
            ..
        } => slice::slice_alpha(&img, &name, *merge_gap, *min_pixels, &o)?,
    };
    if let Some(dir) = &c.frames_dir {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for (i, f) in atlas.frames.iter().enumerate() {
            let p = dir.join(format!("{}.png", f.name));
            atlas
                .extract(&img, i)?
                .save(&p)
                .map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    std::fs::write(&c.out, atlas.to_json()).map_err(|e| format!("{}: {e}", c.out.display()))?;
    println!("{} frames -> {}", atlas.frames.len(), c.out.display());
    Ok(())
}

fn verify_cmd(images: &[PathBuf], palette: &str, limit: usize) -> Result<(), String> {
    let pal = Palette::load(palette)?;
    let mut failed = 0;
    for p in images {
        let img = load_rgba(p)?;
        let off = pal.off_palette(&img);
        if off.is_empty() {
            println!("ok    {} ({}x{})", p.display(), img.width(), img.height());
            continue;
        }
        failed += 1;
        let mut distinct: Vec<[u8; 4]> = off.iter().map(|o| o.rgba).collect();
        distinct.sort_unstable();
        distinct.dedup();
        println!(
            "FAIL  {}: {} off-palette pixel(s), {} distinct colour(s)",
            p.display(),
            off.len(),
            distinct.len()
        );
        let shown = if limit == 0 {
            off.len()
        } else {
            limit.min(off.len())
        };
        for o in &off[..shown] {
            let [r, g, b, a] = o.rgba;
            let [nr, ng, nb] = o.nearest;
            println!(
                "      ({},{}) #{r:02x}{g:02x}{b:02x}{a:02x}  nearest #{nr:02x}{ng:02x}{nb:02x}",
                o.x, o.y
            );
        }
        if shown < off.len() {
            println!("      ... {} more", off.len() - shown);
        }
    }
    if failed > 0 {
        return Err(format!(
            "{failed} of {} image(s) not locked to palette '{}'",
            images.len(),
            pal.name
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn anim_cmd(
    frames: Vec<PathBuf>,
    atlas: Option<PathBuf>,
    image: Option<PathBuf>,
    duration: Vec<u32>,
    scale: u32,
    loops: u16,
    format: Option<Format>,
    palette: Option<String>,
    out: PathBuf,
) -> Result<(), String> {
    let format = format
        .or_else(|| Format::from_path(&out))
        .ok_or("cannot tell format from --out; use .gif/.webp or --format")?;
    let frames = match atlas {
        Some(ap) => {
            let text =
                std::fs::read_to_string(&ap).map_err(|e| format!("{}: {e}", ap.display()))?;
            let mut atlas = Atlas::from_json(&text)?;
            atlas.set_durations(&duration)?;
            let sheet = image
                .unwrap_or_else(|| ap.parent().unwrap_or(Path::new("")).join(&atlas.meta.image));
            atlas.anim_frames(&load_rgba(&sheet)?)?
        }
        None => {
            let ms = expand_durations(
                if duration.is_empty() {
                    &[100]
                } else {
                    &duration
                },
                frames.len(),
            )?;
            frames
                .iter()
                .zip(ms)
                .map(|(p, d)| {
                    Ok(AnimFrame {
                        image: load_rgba(p)?,
                        duration_ms: d,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
        }
    };
    if let Some(name) = palette {
        let pal = Palette::load(&name)?;
        for (i, f) in frames.iter().enumerate() {
            pal.check_image(&f.image)
                .map_err(|e| format!("frame {i}: {e}"))?;
        }
    }
    let bytes = anim::encode(&frames, &AnimOptions { scale, loops }, format)?;
    std::fs::write(&out, bytes).map_err(|e| format!("{}: {e}", out.display()))
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
        Cmd::Verify {
            images,
            palette,
            limit,
        } => verify_cmd(&images, &palette, limit),
        Cmd::Slice { mode } => slice_cmd(mode),
        Cmd::Anim {
            frames,
            atlas,
            image,
            duration,
            scale,
            loops,
            format,
            palette,
            out,
        } => anim_cmd(
            frames, atlas, image, duration, scale, loops, format, palette, out,
        ),
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
