# hypercutter

Extract tiles, palettes, sprites, and field effects from GBA Pokemon ROMs. Ships as a Rust library, a CLI binary, and WebAssembly bindings.

Supports Emerald, FireRed, LeafGreen, Ruby, and Sapphire.

## Features

- Tilesets, metatiles, and palettes (with LZSS decompression)
- Battle sprites: front/back, normal/shiny palette variants, alternate forms
- Overworld character sprites, packed into facing-frames grids
- Field effect sprites (tall grass, surf blob, shadows, etc.) with palettes resolved from the game's script bytecode
- PNG rendering for everything

## Installation

```bash
cargo install hypercutter
```

From source:

```bash
git clone https://github.com/cuebitt/hypercutter && cd hypercutter && cargo build --release
```

## Usage

```bash
hc pokeemerald.gba
```

By default this exports a sprite pack: overworld sprites in a facing-frames grid with `manifest.json`. Field effects go to `out/field_effects/`.

Common flags:

| Flag                        | Description                                                     |
| --------------------------- | --------------------------------------------------------------- |
| `-e, --export <DIR>`        | Output directory (default: `out`)                               |
| `--sym-file <FILE>`         | Custom symbol file (TOML or `.sym`). Bundled tables by default. |
| `-v` / `-q`                 | Verbose / quiet output                                          |
| `-c` / `-y` / `--overwrite` | Output handling (clear, skip prompts, overwrite)                |
| `--tileset-filter <PAT>`    | Glob pattern to filter which tilesets to extract                |
| `--sprite-filter <PAT>`     | Glob pattern to filter which sprites to extract                 |

Symbol tables are bundled as TOML inside the binary. Use `--sym-file` to override for ROM hacks.

## Library

```rust
use hypercutter::{Extractor, Game, Rom, Sprite, SymbolTable, TilesetRenderer};

let rom = Rom::open("pokeemerald.gba")?;
let symbols = SymbolTable::resolve_for_rom(&rom)?;
let extractor = Extractor::new(&rom, &symbols);

// Tilesets
for (name, entry) in extractor.metatiles()?.iter() {
    let mut r = TilesetRenderer::new(&entry.primary);
    if let Some(s) = &entry.secondary { r = r.with_secondary(s); }
    r.render().save_png("...")?;
}

// Sprites and alternate forms
let sprites: Vec<Sprite> = extractor.sprites()?;
let forms = extractor.forms()?;
```

## WebAssembly

See [WASM_JAVASCRIPT.md](WASM_JAVASCRIPT.md) for JS usage, Vite setup, and API reference.

## Development

```bash
# Tests (requires real ROMs in tests/fixtures/)
cargo test --all

# Lint
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked

# Verify packaging
cargo package --list
cargo publish --dry-run

# WASM build
cargo build --release --target wasm32-unknown-unknown
```

### Pre-commit hooks

Configured in `.pre-commit-config.yaml`. Install with:

```bash
cargo install prek --locked
prek install
prek run --all-files
```

If another tool (e.g. bgit) sets `core.hooksPath` globally, try `prek install --git-dir .git` and copy that tool's hooks into `.git/hooks/`. `pre-commit` (pip) works too.

### Publishing

Push a `v*` tag and GitHub Actions publishes to crates.io and npm automatically. Forks need `CRATES_IO_TOKEN` and `NPM_TOKEN` secrets.

```bash
git tag v0.4.0 && git push origin v0.4.0
```

## License

MIT OR Apache-2.0

## Attribution

The TOML memory maps are built from the [pret/pokeemerald](https://github.com/pret/pokeemerald/tree/symbols), [pret/pokefirered](https://github.com/pret/pokefirered/tree/symbols), and [pret/pokeruby](https://github.com/pret/pokeruby/tree/symbols).

This project contains no content from any Pokemon ROM dump.

See [Cargo.toml](Cargo.toml) for the full dependency list.
