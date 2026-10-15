use image::{Rgba, RgbaImage};
use spritegen::{
    dither::{quantize, Dither},
    generate::{generate, Kind},
    sheet, Palette,
};

const KINDS: [Kind; 3] = [Kind::Creature, Kind::Item, Kind::Tile];

#[test]
fn builtins_load() {
    assert_eq!(Palette::builtin("gb4").unwrap().len(), 4);
    assert_eq!(Palette::builtin("pico8").unwrap().len(), 16);
    assert_eq!(Palette::builtin("blackshield").unwrap().len(), 15);
    assert!(Palette::builtin("nope").is_none());
}

#[test]
fn hex_and_gpl_parse() {
    let h = Palette::parse_hex("t", "; c\n#ff0000\n00ff00\n\nFF0000FF\n\nff0000\n").unwrap();
    assert_eq!(h.colors, vec![[255, 0, 0], [0, 255, 0], [0, 0, 255]]);
    assert!(Palette::parse_hex("t", "zzz\n").is_err());
    assert!(Palette::parse_hex("t", "ffffff\n").is_err());
    let g = Palette::parse_gpl(
        "t",
        "GIMP Palette\nName: Duo\nColumns: 2\n#\n  0   0   0\tBlack\n255 255 255 White\n",
    )
    .unwrap();
    assert_eq!(g.name, "Duo");
    assert_eq!(g.colors, vec![[0, 0, 0], [255, 255, 255]]);
    assert!(Palette::parse_gpl("t", "nope\n").is_err());
}

#[test]
fn load_from_file() {
    let dir = std::env::temp_dir().join(format!("psg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("x.gpl");
    std::fs::write(&p, "GIMP Palette\n1 2 3\n4 5 6\n").unwrap();
    assert_eq!(Palette::load(p.to_str().unwrap()).unwrap().len(), 2);
    let p = dir.join("y.hex");
    std::fs::write(&p, "010203\n040506\n070809\n").unwrap();
    assert_eq!(Palette::load(p.to_str().unwrap()).unwrap().len(), 3);
}

#[test]
fn check_image_rejects_foreign_pixels() {
    let pal = Palette::builtin("gb4").unwrap();
    let mut img = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 0]));
    assert!(pal.check_image(&img).is_ok());
    img.put_pixel(0, 0, pal.rgba(1));
    assert!(pal.check_image(&img).is_ok());
    img.put_pixel(1, 1, Rgba([1, 2, 3, 255]));
    assert!(pal.check_image(&img).is_err());
    img.put_pixel(1, 1, Rgba([15, 56, 15, 128])); // semi-transparent not allowed
    assert!(pal.check_image(&img).is_err());
}

#[test]
fn nearest_is_exact_for_palette_colours() {
    let pal = Palette::builtin("pico8").unwrap();
    for (i, c) in pal.colors.iter().enumerate() {
        assert_eq!(pal.nearest(*c), i);
    }
}

#[test]
fn generated_sprites_are_palette_locked_and_mirrored() {
    for pname in spritegen::palette::BUILTINS {
        let pal = Palette::builtin(pname).unwrap();
        for kind in KINDS {
            for size in [8, 16, 24, 32, 48, 64] {
                for seed in 0..20 {
                    let img = generate(kind, size, seed, &pal).unwrap();
                    assert_eq!(img.dimensions(), (size, size));
                    pal.check_image(&img).unwrap();
                    if kind != Kind::Tile {
                        // Shape (alpha) is mirror-symmetric and non-empty.
                        let mut opaque = 0;
                        for y in 0..size {
                            for x in 0..size {
                                let a = img.get_pixel(x, y)[3];
                                assert_eq!(a, img.get_pixel(size - 1 - x, y)[3]);
                                opaque += (a > 0) as u32;
                            }
                        }
                        assert!(
                            opaque > size * size / 8,
                            "{pname} {kind:?} {size} {seed} too empty"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn generation_is_deterministic_and_seed_sensitive() {
    let pal = Palette::builtin("blackshield").unwrap();
    let a = generate(Kind::Creature, 16, 42, &pal).unwrap();
    assert_eq!(a, generate(Kind::Creature, 16, 42, &pal).unwrap());
    assert_ne!(a, generate(Kind::Creature, 16, 43, &pal).unwrap());
}

#[test]
fn bad_sizes_rejected() {
    let pal = Palette::builtin("gb4").unwrap();
    for s in [0, 6, 15, 66, 128] {
        assert!(generate(Kind::Item, s, 1, &pal).is_err());
    }
}

fn gradient(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        Rgba([
            (x * 255 / (w - 1)) as u8,
            (y * 255 / (h - 1)) as u8,
            128,
            if x == 0 && y == 0 { 0 } else { 255 },
        ])
    })
}

#[test]
fn quantize_is_palette_locked_for_all_modes() {
    let src = gradient(40, 30);
    for pname in spritegen::palette::BUILTINS {
        let pal = Palette::builtin(pname).unwrap();
        for d in [Dither::None, Dither::Bayer, Dither::Floyd] {
            let q = quantize(&src, &pal, d);
            pal.check_image(&q).unwrap();
            assert_eq!(q.get_pixel(0, 0)[3], 0, "transparency kept");
        }
    }
}

#[test]
fn quantize_without_dither_is_identity_on_palette_images() {
    let pal = Palette::builtin("pico8").unwrap();
    let img = RgbaImage::from_fn(16, 1, |x, _| pal.rgba(x as usize));
    assert_eq!(quantize(&img, &pal, Dither::None), img);
}

#[test]
fn sheet_layout_and_atlas() {
    let pal = Palette::builtin("gb4").unwrap();
    let frames: Vec<_> = (0..5)
        .map(|i| sheet::Frame {
            name: format!("f{i}"),
            image: generate(Kind::Item, 16, i, &pal).unwrap(),
        })
        .collect();
    let (img, json) = sheet::pack(&frames, 3, 2, "s.png").unwrap();
    assert_eq!(img.dimensions(), (3 * 16 + 2 * 2, 2 * 16 + 2));
    pal.check_image(&img).unwrap();
    assert!(json.contains("\"f4\": { \"frame\": { \"x\": 18, \"y\": 18, \"w\": 16, \"h\": 16 } }"));
    assert!(json.contains("\"count\": 5"));
    assert_eq!(json.matches('{').count(), json.matches('}').count());
    assert!(sheet::pack(&[], 3, 0, "x").is_err());
}
