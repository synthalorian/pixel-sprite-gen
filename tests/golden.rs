//! Golden-image determinism tests. Regenerate intentionally with:
//!   UPDATE_GOLDEN=1 cargo test --test golden
use image::RgbaImage;
use spritegen::{
    dither::{quantize, Dither},
    generate::{generate, Kind},
    Palette,
};
use std::path::PathBuf;

fn check(name: &str, img: &RgbaImage) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        img.save(&path).unwrap();
        return;
    }
    let want = image::open(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (run with UPDATE_GOLDEN=1)", path.display()))
        .to_rgba8();
    assert!(
        want == *img,
        "{name} differs from golden {}",
        path.display()
    );
}

#[test]
fn golden_generated() {
    for (pal, kind, size, seed) in [
        ("blackshield", Kind::Creature, 16, 1),
        ("blackshield", Kind::Item, 16, 2),
        ("gb4", Kind::Creature, 32, 3),
        ("pico8", Kind::Item, 24, 4),
        ("pico8", Kind::Tile, 16, 5),
    ] {
        let p = Palette::builtin(pal).unwrap();
        check(
            &format!("{pal}-{kind:?}-{size}-{seed}").to_lowercase(),
            &generate(kind, size, seed, &p).unwrap(),
        );
    }
}

#[test]
fn golden_quantize() {
    let src = RgbaImage::from_fn(32, 32, |x, y| {
        image::Rgba([(x * 8) as u8, (y * 8) as u8, ((x + y) * 4) as u8, 255])
    });
    for (pal, d) in [
        ("gb4", Dither::Bayer),
        ("gb4", Dither::Floyd),
        ("blackshield", Dither::None),
        ("pico8", Dither::Floyd),
    ] {
        let p = Palette::builtin(pal).unwrap();
        check(
            &format!("quantize-{pal}-{d:?}").to_lowercase(),
            &quantize(&src, &p, d),
        );
    }
}
