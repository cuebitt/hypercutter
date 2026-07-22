# Reference images for visual regression

Drop known-good PNG outputs in this directory, mirroring the CLI's output
layout exactly. For each reference, the path under `tests/reference/` should
match the path the CLI produces under `<export-dir>/`.

## Layout

```
tests/reference/
├── tilesets/
│   ├── General.png            # ← CLI emits: <export-dir>/tilesets/General.png
│   ├── Petalburg.png
│   └── ...
└── pokemon/
    └── sprites/
        ├── 001_bulbasaur/
        │   ├── front.png      # ← CLI emits: <export-dir>/pokemon/sprites/001_bulbasaur/front.png
        │   ├── back.png
        │   ├── front_shiny.png
        │   └── back_shiny.png
        └── ...
```

## Comparing against current output

```sh
# 1. Build and run the CLI to produce current output.
cargo build --release --bin hc
./target/release/hc -y -e /tmp/out tests/fixtures/pokeemerald.gba

# 2. Compare a reference image against the current output.
cargo run --example compare -- \
    tests/reference/tilesets/General.png \
    /tmp/out/tilesets/General.png \
    /tmp/diff.png
```

The comparison tool reports:

- `matching (diff <= 10)`: percentage of pixels whose per-channel sum-of-abs-diffs
  is at most 10 (i.e., close enough: small palette rounding or PNG-encoder
  differences are tolerated).
- `significant (diff > 10)`: percentage of pixels with larger differences.
  These are the candidates for visual inspection.
- `mean abs diff per channel`: per-channel average absolute pixel difference.
  Useful for spotting a global tint or palette-index shift.
- `max diff`: worst per-pixel sum-of-abs-diffs (max 1020 = 4 × 255).
- A diff visualization PNG: transparent = match, yellow = small diff, red = large diff.

## What to look for in the diff image

- **Mostly red, with structure visible**: tile indices or palette indices are off.
- **Yellow everywhere with no clear pattern**: palette rounding (expected to be small).
- **Single solid color offset**: background-pixels look identical but everything
  else is shifted by a constant: likely a palette index translation error.
- **Tiles in wrong slots**: only a few cells in the grid are red: those are
  the metatiles whose source data is wrong.

## Files

Reference PNGs are gitignored alongside `tests/fixtures/*.gba`.
