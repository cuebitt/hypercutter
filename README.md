# hypercutter

Binary extraction tool for GBA Pokemon ROMs. Rust library + CLI + WebAssembly.

## Features

- Extract tiles, palettes, and metatiles from GBA Pokemon ROMs
- Supports Pokemon Emerald, FireRed, LeafGreen, Ruby, Sapphire
- Decompress LZSS-compressed tileset and sprite data
- Render metatiles as PNG images
- Dump Pokemon battle sprites (front, back, normal, shiny, alternate forms)
- Per-form palette and shiny palette support
- CLI and Rust API
- WebAssembly bindings for in-browser use

## Installation

```bash
cargo install hypercutter
```

Or build from source:

```bash
git clone https://github.com/cuebitt/hypercutter
cd hypercutter
cargo build --release
./target/release/hypercutter --help
```

## Usage

```bash
hypercutter pokeemerald.gba
```

Without any flag, both tilesets and sprites are dumped.

Options:

- `-g, --game` — Game to use for symbol file download (`emerald`, `firered`, `leafgreen`, `ruby`, `sapphire`)
- `--sym-file` — Path to a `.sym` file (auto-downloaded if not provided)
- `-e, --export` — Directory to export PNGs (default: `out`)
- `-v, --verbose` — Verbose output
- `-c, --clear` — Clear export directory before writing
- `-y, --yes` — Skip confirmation prompts

Output selection (mutually inclusive with each other; omitting both dumps everything):

- `--tilesets` — Render and export tileset PNGs
- `--sprites` — Dump Pokemon battle sprites

Sprite output options (only meaningful with `--sprites`):

- `--spritesheet` — Output sprites as a spritesheet instead of individual PNGs
- `--spritesheet-columns` — Columns in spritesheet (default: 8)

### Output directory structure

```
out/
├── tilesets/              # One PNG per metatile group
│   ├── General.png
│   ├── Petalburg.png
│   └── ...
└── pokemon/sprites/       # One subdirectory per species
    ├── 001_bulbasaur/
    │   ├── front.png
    │   ├── front_shiny.png
    │   ├── back.png
    │   └── back_shiny.png
    ├── 025_pikachu/
    │   └── ...
    └── ...
```

When alternate forms exist (e.g. Unown, Castform), they are written under a `forms/` subdirectory:

```
    025_pikachu/
    └── forms/
        └── <form-name>/
            ├── front.png
            └── back.png
```

### Symbol file auto-download

The `.sym` file is auto-downloaded from the [pret](https://github.com/pret) disassembly
repositories (via jsDelivr CDN) and cached in your platform's standard cache directory.
Override with the `HYPERCUTTER_CACHE_DIR` environment variable.

## Library

```rust
use hypercutter::{
    Extractor, FormSprite, Game, Rom, SpeciesId, Sprite, SymbolTable,
    TilesetRenderer, SpriteRenderer,
};

let rom = Rom::open("pokeemerald.gba")?;
let symbols = SymbolTable::from_path("pokeemerald.sym")?;
let extractor = Extractor::new(&rom, &symbols);

// Tilesets
let metatiles = extractor.metatiles()?;
for (name, entry) in metatiles.iter() {
    let mut renderer = TilesetRenderer::new(&entry.primary);
    if let Some(secondary) = &entry.secondary {
        renderer = renderer.with_secondary(secondary);
    }
    let img = renderer.render();
    // img.save_png("...")?;
}

// Base species sprites
let sprites: Vec<Sprite> = extractor.sprites()?;
let names = extractor.species_names()?;
let national_dex = extractor.national_dex_map()?;

// Alternate forms
let forms: Vec<FormSprite> = extractor.forms()?;
```

## Development

```bash
# Run tests (requires real ROMs in tests/fixtures/)
cargo test --all

# Lint
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked

# Build for WebAssembly
cargo build --release --target wasm32-unknown-unknown
```

Pre-commit hooks for `cargo fmt` and `cargo clippy` are configured in
`.pre-commit-config.yaml`.

## License

MIT OR Apache-2.0

## Attribution

- [magical/nlzss](https://github.com/magical/nlzss) (decompression via [nintendo-lz](https://crates.io/crates/nintendo-lz))
- [pret/pokeemerald](https://github.com/pret/pokeemerald/tree/symbols), [pret/pokefirered](https://github.com/pret/pokefirered/tree/symbols), [pret/pokeruby](https://github.com/pret/pokeruby/tree/symbols)
  - Only the memory maps are used. This project contains no content from any Pokemon ROM dump.
