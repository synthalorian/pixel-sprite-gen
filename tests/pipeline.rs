//! Slicing, palette verification and animation export. All images are small geometric shapes
//! drawn in code.
use image::{Rgba, RgbaImage};
use spritegen::{
    anim::{self, AnimFrame, AnimOptions, Format},
    atlas::{Atlas, Pivot, Rect},
    dither::{quantize, Dither},
    slice::{self, Grid, SliceOptions},
    Palette,
};
use std::path::Path;

const CLEAR: Rgba<u8> = Rgba([0, 0, 0, 0]);

fn gb4() -> Palette {
    Palette::builtin("gb4").unwrap()
}

fn fill(img: &mut RgbaImage, r: Rect, c: Rgba<u8>) {
    for y in r.y..r.bottom() {
        for x in r.x..r.right() {
            img.put_pixel(x, y, c);
        }
    }
}

fn disc(img: &mut RgbaImage, cx: i32, cy: i32, rad: i32, c: Rgba<u8>) {
    for y in cy - rad..=cy + rad {
        for x in cx - rad..=cx + rad {
            if (x - cx).pow(2) + (y - cy).pow(2) <= rad * rad {
                img.put_pixel(x as u32, y as u32, c);
            }
        }
    }
}

/// `n` frames of a 2x2 square moving diagonally across an `s`×`s` transparent canvas.
fn moving_square(n: u32, s: u32, pal: &Palette) -> Vec<AnimFrame> {
    (0..n)
        .map(|i| {
            let mut img = RgbaImage::from_pixel(s, s, CLEAR);
            fill(
                &mut img,
                Rect::new(i % (s - 1), i % (s - 1), 2, 2),
                pal.rgba(1 + i as usize % 3),
            );
            img.put_pixel(s - 1, 0, pal.rgba(0)); // fixed opaque marker
            AnimFrame {
                image: img,
                duration_ms: 40 + 10 * i,
            }
        })
        .collect()
}

// ---------- slicing ----------

#[test]
fn grid_cells_respect_margin_and_spacing() {
    let g = Grid {
        cell_w: 8,
        cell_h: 6,
        margin: 1,
        spacing: 2,
    };
    // 1 + 8 + 2 + 8 + 2 + 8 + 1 = 30 wide → 3 columns; 1 + 6 + 2 + 6 + 1 = 16 (+3 spare) → 2 rows.
    let cells = slice::grid_cells(30, 19, &g).unwrap();
    assert_eq!(cells.len(), 6);
    assert_eq!(cells[0], Rect::new(1, 1, 8, 6));
    assert_eq!(cells[2], Rect::new(21, 1, 8, 6));
    assert_eq!(cells[4], Rect::new(11, 9, 8, 6));
    assert!(
        slice::grid_cells(29, 19, &g).unwrap().len() == 4,
        "partial column ignored"
    );
    assert!(slice::grid_cells(5, 5, &g).is_err());
    assert!(slice::grid_cells(30, 19, &Grid { cell_w: 0, ..g }).is_err());
}

#[test]
fn grid_cells_reject_overflowing_spacing() {
    let g = Grid {
        cell_w: 1,
        cell_h: 1,
        margin: 0,
        spacing: u32::MAX,
    };
    assert!(slice::grid_cells(8, 8, &g).is_err());
    let huge_margin = Grid {
        margin: u32::MAX,
        spacing: 0,
        ..g
    };
    assert!(slice::grid_cells(8, 8, &huge_margin).is_err());
}

fn grid_sheet() -> RgbaImage {
    // 3x1 cells of 8x8, margin 1, spacing 2. Cell 1 is empty.
    let pal = gb4();
    let mut img = RgbaImage::from_pixel(30, 10, CLEAR);
    fill(&mut img, Rect::new(1 + 2, 1 + 3, 3, 4), pal.rgba(1)); // cell 0 at (1,1)
    fill(&mut img, Rect::new(21, 1, 8, 8), pal.rgba(2)); // cell 2 at (21,1), full
    img
}

#[test]
fn grid_slice_skips_empty_and_trims() {
    let img = grid_sheet();
    let g = Grid {
        cell_w: 8,
        cell_h: 8,
        margin: 1,
        spacing: 2,
    };
    let o = SliceOptions {
        prefix: "s".into(),
        pivot: Pivot { x: 0.5, y: 1.0 },
        durations: vec![80],
        ..Default::default()
    };

    let a = slice::slice_grid(&img, "sheet.png", &g, false, false, &o).unwrap();
    assert_eq!(a.frames.len(), 2);
    assert_eq!(a.frames[0].frame, Rect::new(1, 1, 8, 8));
    assert_eq!(a.frames[1].name, "s_1");
    assert!(a.frames.iter().all(|f| !f.trimmed && f.duration == 80));

    let a = slice::slice_grid(&img, "sheet.png", &g, false, true, &o).unwrap();
    assert_eq!(a.frames.len(), 3, "keep_empty");

    let a = slice::slice_grid(&img, "sheet.png", &g, true, false, &o).unwrap();
    let f = &a.frames[0];
    assert!(f.trimmed);
    assert_eq!(f.frame, Rect::new(3, 4, 3, 4));
    assert_eq!(f.sprite_source_size, Rect::new(2, 3, 3, 4));
    assert_eq!((f.source_size.w, f.source_size.h), (8, 8));
    assert!(!a.frames[1].trimmed, "full cell is not trimmed");
    // Extraction restores the original cell exactly.
    let cell0 = image::imageops::crop_imm(&img, 1, 1, 8, 8).to_image();
    assert_eq!(a.extract(&img, 0).unwrap(), cell0);
}

#[test]
fn alpha_slice_finds_regions_in_reading_order() {
    let pal = gb4();
    let mut img = RgbaImage::from_pixel(40, 30, CLEAR);
    fill(&mut img, Rect::new(20, 3, 6, 4), pal.rgba(1)); // row 1, right, short
    disc(&mut img, 6, 6, 4, pal.rgba(2)); // row 1, left, taller (2..=10)
                                          // 5x5 ring with a detached centre dot: the dot's box lies inside the ring's, so they merge.
    for i in 0..5 {
        for (x, y) in [(30 + i, 1), (30 + i, 5), (30, 1 + i), (34, 1 + i)] {
            img.put_pixel(x, y, pal.rgba(3));
        }
    }
    img.put_pixel(32, 3, pal.rgba(1));
    fill(&mut img, Rect::new(2, 18, 5, 5), pal.rgba(3)); // row 2
    img.put_pixel(30, 25, pal.rgba(1)); // single-pixel speck

    let o = SliceOptions {
        prefix: "a".into(),
        ..Default::default()
    };
    let a = slice::slice_alpha(&img, "x.png", 0, 2, &o).unwrap();
    let rects: Vec<Rect> = a.frames.iter().map(|f| f.frame).collect();
    assert_eq!(
        rects,
        vec![
            Rect::new(2, 2, 9, 9),
            Rect::new(20, 3, 6, 4),
            Rect::new(30, 1, 5, 5),
            Rect::new(2, 18, 5, 5)
        ]
    );
    assert!(a
        .frames
        .iter()
        .all(|f| !f.trimmed && f.sprite_source_size == Rect::new(0, 0, f.frame.w, f.frame.h)));

    // The speck survives with min_pixels 1.
    assert_eq!(
        slice::slice_alpha(&img, "x.png", 0, 1, &o)
            .unwrap()
            .frames
            .len(),
        5
    );
    // Nothing solid → error.
    assert!(slice::slice_alpha(&RgbaImage::new(4, 4), "x", 0, 1, &o).is_err());
}

#[test]
fn merge_gap_counts_empty_pixels_between_boxes() {
    let mut img = RgbaImage::from_pixel(12, 4, CLEAR);
    fill(&mut img, Rect::new(0, 0, 3, 3), Rgba([9, 9, 9, 255]));
    fill(&mut img, Rect::new(6, 1, 3, 3), Rgba([9, 9, 9, 255])); // 3 empty columns between
    assert_eq!(slice::connected_regions(&img, 0, 2, 1).len(), 2);
    assert_eq!(
        slice::connected_regions(&img, 0, 3, 1),
        vec![Rect::new(0, 0, 9, 4)]
    );
    // Touching boxes of separate regions (diagonal gap) stay apart at gap 0, merge at gap 1.
    let mut img = RgbaImage::from_pixel(6, 6, CLEAR);
    fill(&mut img, Rect::new(0, 0, 2, 2), Rgba([9, 9, 9, 255]));
    fill(&mut img, Rect::new(2, 3, 2, 2), Rgba([9, 9, 9, 255]));
    assert_eq!(slice::connected_regions(&img, 0, 0, 1).len(), 2);
    assert_eq!(slice::connected_regions(&img, 0, 1, 1).len(), 1);
}

#[test]
fn alpha_threshold_ignores_faint_pixels() {
    let mut img = RgbaImage::from_pixel(10, 10, CLEAR);
    fill(&mut img, Rect::new(1, 1, 3, 3), Rgba([255, 0, 0, 255]));
    fill(&mut img, Rect::new(6, 6, 3, 3), Rgba([255, 0, 0, 40]));
    assert_eq!(slice::connected_regions(&img, 0, 0, 1).len(), 2);
    assert_eq!(
        slice::connected_regions(&img, 64, 0, 1),
        vec![Rect::new(1, 1, 3, 3)]
    );
}

#[test]
fn atlas_json_round_trip_and_durations() {
    let img = grid_sheet();
    let g = Grid {
        cell_w: 8,
        cell_h: 8,
        margin: 1,
        spacing: 2,
    };
    let o = SliceOptions {
        durations: vec![50, 120],
        ..Default::default()
    };
    let a = slice::slice_grid(&img, "sheet.png", &g, true, false, &o).unwrap();
    assert_eq!(
        a.frames.iter().map(|f| f.duration).collect::<Vec<_>>(),
        vec![50, 120]
    );
    let json = a.to_json();
    for key in [
        "\"filename\": \"frame_1\"",
        "\"spriteSourceSize\"",
        "\"sourceSize\"",
        "\"pivot\"",
        "\"duration\": 120",
        "\"image\": \"sheet.png\"",
    ] {
        assert!(json.contains(key), "missing {key}");
    }
    assert_eq!(Atlas::from_json(&json).unwrap(), a);
    assert!(!json.contains("\"name\""));
    assert!(Atlas::from_json(&json.replace("\"w\": 3", "\"w\": 30")).is_err());
    // Atlases written with the old `name` key still load.
    let legacy = json.replace("\"filename\":", "\"name\":");
    assert_eq!(Atlas::from_json(&legacy).unwrap(), a);

    let three = SliceOptions {
        durations: vec![1, 2, 3],
        ..Default::default()
    };
    assert!(slice::slice_grid(&img, "s", &g, false, false, &three).is_err());
}

#[test]
fn pivot_parsing() {
    assert_eq!("0.5,1".parse::<Pivot>().unwrap(), Pivot { x: 0.5, y: 1.0 });
    assert!("0.5".parse::<Pivot>().is_err());
    assert!("1.5,0".parse::<Pivot>().is_err());
    assert!("a,b".parse::<Pivot>().is_err());
}

#[test]
fn anim_frames_align_pivots() {
    // Two bars of different heights standing on the same baseline (pivot bottom-centre).
    let pal = gb4();
    let mut sheet = RgbaImage::from_pixel(12, 8, CLEAR);
    fill(&mut sheet, Rect::new(0, 0, 4, 8), pal.rgba(1));
    fill(&mut sheet, Rect::new(6, 4, 6, 4), pal.rgba(2));
    let o = SliceOptions {
        pivot: Pivot { x: 0.5, y: 1.0 },
        durations: vec![70],
        ..Default::default()
    };
    let a = slice::slice_alpha(&sheet, "s", 0, 1, &o).unwrap();
    let frames = a.anim_frames(&sheet).unwrap();
    assert_eq!(frames.len(), 2);
    for f in &frames {
        assert_eq!(f.image.dimensions(), (6, 8));
        assert_eq!(f.duration_ms, 70);
    }
    // Tall bar: columns 1..5, all rows. Wide bar: all columns, rows 4..8.
    assert_eq!(alpha_box(&frames[0].image), Rect::new(1, 0, 4, 8));
    assert_eq!(alpha_box(&frames[1].image), Rect::new(0, 4, 6, 4));
}

fn alpha_box(img: &RgbaImage) -> Rect {
    slice::alpha_bounds(img, Rect::new(0, 0, img.width(), img.height()), 0).unwrap()
}

// ---------- palette lock ----------

#[test]
fn off_palette_lists_every_offending_pixel() {
    let pal = gb4();
    let mut img = RgbaImage::from_pixel(4, 3, CLEAR);
    img.put_pixel(0, 0, pal.rgba(0));
    img.put_pixel(2, 1, Rgba([0x10, 0x39, 0x10, 255])); // near-miss of gb4[0]
    img.put_pixel(3, 2, Rgba([0x9b, 0xbc, 0x0f, 200])); // palette colour, partial alpha
    let off = pal.off_palette(&img);
    assert_eq!(off.len(), 2);
    assert_eq!(
        (off[0].x, off[0].y, off[0].nearest),
        (2, 1, [0x0f, 0x38, 0x0f])
    );
    assert_eq!((off[1].x, off[1].y, off[1].rgba[3]), (3, 2, 200));
    assert!(pal.check_image(&img).is_err());
}

#[test]
fn quantize_from_gpl_palette_then_verifies() {
    let pal = Palette::parse_gpl(
        "rgb",
        "GIMP Palette\nName: rgb\n255 0 0\n0 255 0\n0 0 255\n0 0 0\n",
    )
    .unwrap();
    let src = RgbaImage::from_fn(12, 12, |x, y| {
        Rgba([(x * 21) as u8, (y * 21) as u8, ((x + y) * 10) as u8, 255])
    });
    assert!(!pal.off_palette(&src).is_empty());
    for d in [Dither::None, Dither::Bayer, Dither::Floyd] {
        assert!(pal.off_palette(&quantize(&src, &pal, d)).is_empty());
    }
}

// ---------- animation ----------

#[test]
fn nearest_scale_is_exact() {
    let src = RgbaImage::from_fn(3, 2, |x, y| Rgba([x as u8, y as u8, 7, 255]));
    let s = anim::scale_nearest(&src, 4);
    assert_eq!(s.dimensions(), (12, 8));
    for (x, y, p) in s.enumerate_pixels() {
        assert_eq!(p, src.get_pixel(x / 4, y / 4));
    }
    assert_eq!(anim::scale_nearest(&src, 1), src);
}

#[test]
fn gif_delay_rounding() {
    assert_eq!(anim::gif_delay_cs(100), 10);
    assert_eq!(anim::gif_delay_cs(84), 8);
    assert_eq!(anim::gif_delay_cs(85), 9);
    assert_eq!(anim::gif_delay_cs(0), 2);
    assert_eq!(anim::gif_delay_cs(u32::MAX), u16::MAX);
}

fn decode_gif(bytes: &[u8]) -> (gif::Repeat, Vec<(u16, RgbaImage)>) {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut d = opts.read_info(bytes).unwrap();
    let (w, h) = (d.width() as u32, d.height() as u32);
    let mut frames = Vec::new();
    while let Some(f) = d.read_next_frame().unwrap() {
        assert_eq!(
            (f.width as u32, f.height as u32, f.left, f.top),
            (w, h, 0, 0)
        );
        frames.push((
            f.delay,
            RgbaImage::from_raw(w, h, f.buffer.to_vec()).unwrap(),
        ));
    }
    (d.repeat(), frames)
}

#[test]
fn gif_round_trip_is_exact() {
    let pal = gb4();
    let frames = moving_square(4, 6, &pal);
    let bytes = anim::encode_gif(&frames, &AnimOptions { scale: 3, loops: 0 }).unwrap();
    let (repeat, got) = decode_gif(&bytes);
    assert_eq!(repeat, gif::Repeat::Infinite);
    assert_eq!(got.len(), 4);
    for ((delay, img), f) in got.iter().zip(&frames) {
        assert_eq!(*delay, anim::gif_delay_cs(f.duration_ms));
        assert_eq!(img, &anim::scale_nearest(&f.image, 3));
        pal.check_image(img).unwrap();
    }
}

#[test]
fn gif_loop_counts() {
    let frames = moving_square(2, 4, &gb4());
    let rep =
        |loops| decode_gif(&anim::encode_gif(&frames, &AnimOptions { scale: 1, loops }).unwrap()).0;
    assert_eq!(
        rep(1),
        gif::Repeat::Finite(0),
        "play once: no loop extension"
    );
    assert_eq!(rep(3), gif::Repeat::Finite(2));
}

#[test]
fn gif_rejects_too_many_colours() {
    let opaque = |n: u32| {
        vec![AnimFrame {
            image: RgbaImage::from_fn(n, 1, |x, _| Rgba([x as u8, (x >> 8) as u8, 9, 255])),
            duration_ms: 100,
        }]
    };
    assert!(anim::encode_gif(&opaque(256), &AnimOptions::default()).is_ok());
    assert!(anim::encode_gif(&opaque(257), &AnimOptions::default()).is_err());
    let mut f = opaque(256);
    f[0].image.put_pixel(0, 0, CLEAR);
    f[0].image.put_pixel(1, 0, CLEAR); // 254 colours + transparency fits
    assert!(anim::encode_gif(&f, &AnimOptions::default()).is_ok());
    let base = opaque(256).remove(0).image;
    f[0].image = RgbaImage::from_fn(257, 1, |x, _| {
        if x == 256 {
            CLEAR
        } else {
            *base.get_pixel(x, 0)
        }
    });
    assert!(
        anim::encode_gif(&f, &AnimOptions::default()).is_err(),
        "256 colours + transparency"
    );
}

#[test]
fn webp_round_trip_is_exact() {
    let pal = gb4();
    let frames = moving_square(5, 7, &pal);
    for (loops, want) in [
        (0u16, image_webp::LoopCount::Forever),
        (3, image_webp::LoopCount::Times(3.try_into().unwrap())),
    ] {
        let bytes = anim::encode_webp(&frames, &AnimOptions { scale: 2, loops }).unwrap();
        let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).unwrap();
        assert!(d.is_animated() && d.has_alpha());
        assert_eq!(d.dimensions(), (14, 14));
        assert_eq!(d.num_frames(), 5);
        assert_eq!(d.loop_count(), want);
        let mut buf = vec![0; d.output_buffer_size().unwrap()];
        for f in &frames {
            let ms = d.read_frame(&mut buf).unwrap();
            assert_eq!(ms, f.duration_ms);
            assert_eq!(
                RgbaImage::from_raw(14, 14, buf.clone()).unwrap(),
                anim::scale_nearest(&f.image, 2)
            );
        }
    }
}

#[test]
fn webp_decodes_through_image_crate() {
    use image::AnimationDecoder;
    let frames = moving_square(3, 5, &gb4());
    let bytes = anim::encode(&frames, &AnimOptions::default(), Format::Webp).unwrap();
    let dec = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(bytes)).unwrap();
    let got = dec.into_frames().collect_frames().unwrap();
    assert_eq!(got.len(), 3);
    assert_eq!(got[2].buffer(), &frames[2].image);
}

#[test]
fn encoders_validate_input() {
    let frames = moving_square(2, 4, &gb4());
    let mut mixed = frames.clone();
    mixed[1].image = RgbaImage::new(5, 4);
    for fmt in [Format::Gif, Format::Webp] {
        assert!(anim::encode(&[], &AnimOptions::default(), fmt).is_err());
        assert!(anim::encode(&mixed, &AnimOptions::default(), fmt).is_err());
        assert!(anim::encode(&frames, &AnimOptions { scale: 0, loops: 0 }, fmt).is_err());
    }
    assert!(anim::encode_webp(
        &frames,
        &AnimOptions {
            scale: 4097,
            loops: 0
        }
    )
    .is_err());
    assert!(anim::encode_gif(
        &frames,
        &AnimOptions {
            scale: 16384,
            loops: 0
        }
    )
    .is_err());
    assert_eq!(Format::from_path(Path::new("a/b.GIF")), Some(Format::Gif));
    assert_eq!(Format::from_path(Path::new("b.webp")), Some(Format::Webp));
    assert_eq!(Format::from_path(Path::new("b.png")), None);
}

// ---------- CLI ----------

fn sprite(args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_sprite"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn cli_slice_verify_anim_pipeline() {
    let dir = std::env::temp_dir().join(format!("psg-pipeline-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let p = |n: &str| dir.join(n).to_str().unwrap().to_string();

    // 4 frames of 6x6 in a row with 1 px spacing.
    let pal = gb4();
    let mut sheet = RgbaImage::from_pixel(27, 6, CLEAR);
    for (i, f) in moving_square(4, 6, &pal).iter().enumerate() {
        image::imageops::replace(&mut sheet, &f.image, i as i64 * 7, 0);
    }
    sheet.save(p("sheet.png")).unwrap();

    let out = sprite(&["verify", &p("sheet.png"), "--palette", "gb4"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let mut bad = sheet.clone();
    bad.put_pixel(3, 3, Rgba([1, 2, 3, 255]));
    bad.save(p("bad.png")).unwrap();
    let out = sprite(&["verify", &p("sheet.png"), &p("bad.png"), "--palette", "gb4"]);
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("(3,3) #010203ff"), "{stdout}");

    let out = sprite(&[
        "slice",
        "grid",
        &p("sheet.png"),
        "--cell",
        "6",
        "--spacing",
        "1",
        "--duration",
        "60,70,80,90",
        "--pivot",
        "0.5,1",
        "--frames-dir",
        &p("frames"),
        "-o",
        &p("sheet.json"),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = sprite(&[
        "slice",
        "grid",
        &p("sheet.png"),
        "--cell",
        "1",
        "--spacing",
        "4294967295",
        "-o",
        &p("overflow.json"),
    ]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("overflows"), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");

    let atlas = Atlas::from_json(&std::fs::read_to_string(p("sheet.json")).unwrap()).unwrap();
    assert_eq!(atlas.meta.image, "sheet.png");
    assert_eq!(atlas.frames.len(), 4);
    assert!(dir.join("frames/sheet_3.png").exists());

    let out = sprite(&[
        "anim",
        "--atlas",
        &p("sheet.json"),
        "--scale",
        "2",
        "--palette",
        "gb4",
        "-o",
        &p("a.gif"),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (_, frames) = decode_gif(&std::fs::read(p("a.gif")).unwrap());
    assert_eq!(
        frames.iter().map(|f| f.0).collect::<Vec<_>>(),
        vec![6, 7, 8, 9]
    );
    assert_eq!(frames[0].1.dimensions(), (12, 12));

    let f0 = p("frames/sheet_0.png");
    let f1 = p("frames/sheet_1.png");
    let out = sprite(&["anim", &f0, &f1, "--loops", "2", "-o", &p("a.webp")]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let d = image_webp::WebPDecoder::new(std::io::Cursor::new(std::fs::read(p("a.webp")).unwrap()))
        .unwrap();
    assert_eq!(d.num_frames(), 2);

    let out = sprite(&["anim", &p("bad.png"), "--palette", "gb4", "-o", &p("x.gif")]);
    assert!(!out.status.success(), "palette check enforced");
    let out = sprite(&["anim", &f0, "-o", &p("x.png")]);
    assert!(!out.status.success(), "unknown format");
    std::fs::remove_dir_all(&dir).unwrap();
}
