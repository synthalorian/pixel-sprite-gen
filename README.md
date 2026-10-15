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

## Tests

`tests/core.rs` covers palette parsing, the in-palette check, mirroring and palette-locking across
every built-in palette × kind × size × 20 seeds, dithering, and sheet layout. `tests/golden.rs`
compares output with the PNGs in `tests/golden/`; after an intentional change to the generator,
regenerate them with `UPDATE_GOLDEN=1 cargo test --test golden` and review the diff.

## Licence

Proprietary. See `LICENSE`.
