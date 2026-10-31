# pixel-sprite-gen

Palette-locked pixel-art sprite generator. Every pixel it writes is either fully transparent
or an exact colour from the chosen palette, and every output is checked before saving.

- **Palettes:** built-in `blackshield` (15 colours, from the Blackshield Omarchy theme), `gb4`
  (Game Boy 4-shade), `pico8` (PICO-8 16), or any Lospec `.hex` / GIMP `.gpl` file.
- **Procedural sprites:** `creature`, `item`, `tile`, 8–64 px (even sizes). Mask template +
  seeded ChaCha noise + mirror + outline + top-light shading. Same seed, same sprite.
- **Quantize:** map any PNG onto a palette with no dither, ordered Bayer 4×4, or
  Floyd–Steinberg. Colour matching and error diffusion run in OKLab.
- **Sheets:** N variants by consecutive seeds packed into a PNG plus a JSON atlas
  (TexturePacker-style `frames` hash, with `meta.frame_size` for grid loaders).
- **Slicing:** cut any sprite sheet into frames by grid (cell size, margin, spacing, optional
  per-cell alpha trim) or by connected alpha regions, emitting an ordered frame atlas with
  pivots and per-frame durations.
- **Palette lock:** `verify` lists every off-palette pixel (with the nearest palette colour) and
  exits non-zero; `quantize` maps images onto a palette with optional dithering.
- **Animation:** animated GIF and lossless animated WebP with integer nearest-neighbour scale-up,
  per-frame timing and loop count.

## Build

```sh
cargo build --release      # binary: target/release/sprite
cargo test
```

## Usage

```sh
sprite gen --kind creature --size 32 --seed 7 --palette blackshield --scale 8 -o creature.png
sprite sheet --kind item --size 16 --count 16 --cols 8 --pad 1 --palette pico8 -o items.png   # + items.json
sprite quantize logo.png --palette gb4 --dither bayer --max-size 64 -o logo-gb.png
sprite palette             # list built-ins
sprite palette my.gpl      # print a palette's colours
```

`--scale` is a nearest-neighbour preview upscale; leave it at 1 for game assets.

### Palette lock

```sh
sprite verify walk.png idle.png --palette my.gpl   # exit 1 if any pixel is off-palette
sprite quantize walk.png --palette my.gpl --dither none -o walk-locked.png
```

A pixel passes when it is fully transparent or an exact, fully opaque palette colour. Failures
print one line per offending pixel (`--limit N`, default 20; `0` lists all):

```
FAIL  walk.png: 2 off-palette pixel(s), 1 distinct colour(s)
      (3,3) #010203ff  nearest #0f380f
```

Palettes are a built-in name, a Lospec `.hex` file (one `RRGGBB` per line) or a GIMP `.gpl` file.
`--dither` is `none`, `bayer` (ordered 4×4, default) or `floyd` (Floyd–Steinberg).

### Slicing

```sh
# Grid: 16x16 cells, 1 px border, 2 px gutters, crop each frame to its pixels, feet as pivot.
sprite slice grid walk.png --cell 16x16 --margin 1 --spacing 2 --trim --pivot 0.5,1 \
    --duration 100 -o walk.json
# Alpha: one frame per connected opaque region; merge parts up to 2 px apart.
sprite slice alpha loose.png --merge-gap 2 --min-pixels 4 --duration 80,80,120 -o loose.json
```

- Grid cells run left to right, top to bottom; partial cells at the right/bottom edge are ignored
  and fully transparent cells are skipped unless `--keep-empty`.
- Alpha regions are 8-connected. Regions whose bounding boxes overlap always merge (so a ring and
  its centre dot stay one frame); `--merge-gap N` also merges boxes up to N empty pixels apart.
  Frames are ordered in rows: a new row starts when a box begins below every box in the
  current row.
- `--duration` takes one value for every frame or exactly one per frame. `--alpha-threshold`
  (default 0) sets the alpha above which a pixel counts as opaque.
- `--frames-dir DIR` also writes each frame, untrimmed, as `DIR/<filename>.png`.

The atlas follows TexturePacker's JSON-array layout plus `duration`. Array order is
playback order. Frames are keyed by `filename` (atlases using `name` are still read). `meta.image` is the sheet path relative to the atlas file.

```json
{
  "frames": [
    {
      "filename": "walk_0",
      "frame": { "x": 3, "y": 4, "w": 10, "h": 12 },
      "trimmed": true,
      "spriteSourceSize": { "x": 2, "y": 3, "w": 10, "h": 12 },
      "sourceSize": { "w": 16, "h": 16 },
      "pivot": { "x": 0.5, "y": 1.0 },
      "duration": 100
    }
  ],
  "meta": { "app": "pixel-sprite-gen", "version": "0.1.0", "image": "walk.png", "size": { "w": 54, "h": 18 } }
}
```

`frame` is the region in the sheet; `spriteSourceSize` is where it sits in the untrimmed
`sourceSize` rectangle. `pivot` is normalised against `sourceSize` (0,0 top-left, 1,1
bottom-right), so trimming never moves it.

### Animation

```sh
sprite anim --atlas walk.json --scale 4 -o walk.gif                 # durations from the atlas
sprite anim --atlas walk.json --scale 4 --loops 3 -o walk.webp
sprite anim f0.png f1.png f2.png --duration 120,80,80 --palette gb4 -o blink.gif
```

- Format comes from the `--out` extension (`.gif` / `.webp`) or `--format`.
- `--scale K` replicates every pixel into a K×K block; there is no filtering.
- `--loops N` is the total number of plays; `0` (default) loops forever.
- `--duration` overrides atlas durations; frame PNGs default to 100 ms.
- `--palette` refuses to export unless every frame is palette-locked.
- With `--atlas`, frames are placed on one shared canvas with their pivots aligned, so
  frames of different sizes (trimmed or alpha-sliced) don't jitter. The canvas is the
  smallest one that holds every frame. Frame PNGs must all be the same size.

**GIF notes.** Colours are stored exactly in one global colour table; nothing is quantised, so
palette-locked frames stay palette-locked. GIF holds at most 256 colours (255 when any pixel is
transparent); if you have more, run `quantize` first. Alpha below 128 becomes transparent and
anything else is opaque. Delays are stored in 10 ms units, so durations are rounded to the
nearest 10 ms and raised to at least 20 ms (browsers play shorter delays at 100 ms). A loop count
of 1 writes no loop extension; N > 1 writes N − 1 repeats.

**WebP notes.** Output is lossless and keeps full 8-bit alpha. Durations are exact milliseconds
(up to 16,777,215). The canvas can be at most 16384 px on a side. The pure-Rust `image-webp`
encoder only writes still images, so each frame is encoded as a lossless VP8L still and this crate
writes the animated container (`VP8X` / `ANIM` / `ANMF`) itself. No C libwebp is needed. Every
frame is stored at full canvas size, so files are larger than libwebp's delta-optimised output.
Output decodes with `image-webp` (used in the tests) and libwebp (checked with Pillow).

`sprite sheet` writes the older frames-hash atlas. `anim --atlas` reads only the array format, so
run a generated sheet through `slice grid` first:

```sh
sprite sheet --kind creature --size 16 --count 4 --cols 4 --palette gb4 -o idle.png
sprite slice grid idle.png --cell 16 --duration 150 -o idle-anim.json
sprite anim --atlas idle-anim.json --scale 6 -o idle.gif
```

### Atlas format

```json
{
  "frames": {
    "creature_1": { "frame": { "x": 0, "y": 0, "w": 16, "h": 16 } }
  },
  "meta": { "image": "sheet.png", "size": { "w": 128, "h": 32 }, "frame_size": { "w": 16, "h": 16 }, "count": 16 }
}
```

## Library

```rust
use spritegen::{generate::{generate, Kind}, Palette};
let pal = Palette::load("gb4")?;
let img = generate(Kind::Creature, 16, 42, &pal)?;
pal.check_image(&img)?;
```

Slice, verify and animate:

```rust
use spritegen::{anim::{self, AnimOptions, Format}, slice::{self, Grid, SliceOptions}, Palette};
let sheet = image::open("walk.png")?.to_rgba8();
let grid = Grid { cell_w: 16, cell_h: 16, margin: 0, spacing: 0 };
let atlas = slice::slice_grid(&sheet, "walk.png", &grid, true, false, &SliceOptions::default())?;
std::fs::write("walk.json", atlas.to_json())?;

let pal = Palette::load("palettes/my.gpl")?;
for off in pal.off_palette(&sheet) {
    eprintln!("({},{}) {:?} -> nearest {:?}", off.x, off.y, off.rgba, off.nearest);
}

let frames = atlas.anim_frames(&sheet)?; // pivot-aligned, durations from the atlas
let gif = anim::encode(&frames, &AnimOptions { scale: 4, loops: 0 }, Format::Gif)?;
```

| Module | Purpose |
| --- | --- |
| `palette` | load `.hex` / `.gpl`, nearest colour, `check_image`, `off_palette` |
| `dither` | `quantize` with `None` / `Bayer` / `Floyd` |
| `slice` | `grid_cells`, `alpha_bounds`, `connected_regions`, `slice_grid`, `slice_alpha` |
| `atlas` | `Atlas` JSON (de)serialisation, `extract`, pivot-aligned `anim_frames` |
| `anim` | `scale_nearest`, `encode_gif`, `encode_webp`, `encode` |

## Tests

`tests/core.rs` covers palette parsing, the in-palette check, mirroring and palette-locking across
every built-in palette × kind × size × 20 seeds, dithering, and sheet layout. `tests/golden.rs`
compares output with the PNGs in `tests/golden/`; after an intentional change to the generator,
regenerate them with `UPDATE_GOLDEN=1 cargo test --test golden` and review the diff.
`tests/pipeline.rs` covers grid and alpha slicing, atlas round-trips, pivot alignment, off-palette
reporting, and exact GIF / WebP round-trips (pixels, delays, loop counts). It also runs the CLI
end to end: verify, then slice, then anim. All test images are small geometric shapes drawn in code.

## Licence

Proprietary. See `LICENSE`.
