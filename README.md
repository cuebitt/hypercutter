# hypercutter

Binary extraction tool for GBA Pokemon ROMs. Includes a Rust library, a CLI, and WebAssembly bindings.

## Features

- Extract tiles, palettes, metatiles, and battle sprites (front/back, normal/shiny, alternate forms) from Emerald, FireRed, LeafGreen, Ruby, Sapphire
- LZSS decompression, PNG rendering, per-form palettes

## Installation

```bash
cargo install hypercutter
```

Or from source:

```bash
git clone https://github.com/cuebitt/hypercutter && cd hypercutter && cargo build --release
```

## Usage

```bash
hc pokeemerald.gba
```

Exports a sprite pack by default: field/overworld sprites in a facing-frames grid with `manifest.json`.

Common flags:

| Flag                        | Description                                                                    |
| --------------------------- | ------------------------------------------------------------------------------ |
| `-e, --export <DIR>`        | Output directory (default: `out`)                                              |
| `--flat`                    | Flat output (tileset PNGs + sprite PNGs). Implied by `--tilesets`/`--sprites`. |
| `--sym-file <FILE>`         | Custom symbol file (TOML or `.sym`). Bundled tables used by default.           |
| `-v` / `-q`                 | Verbose / quiet output                                                         |
| `-c` / `-y` / `--overwrite` | Output handling (clear, skip prompts, overwrite)                               |

Flat-mode options (with `--flat`):

| Flag                                          | Description                         |
| --------------------------------------------- | ----------------------------------- |
| `--tilesets` / `--sprites`                    | What to extract                     |
| `--tileset-filter` / `--sprite-filter <PAT>`  | Glob pattern filter                 |
| `--spritesheet` / `--spritesheet-columns <N>` | Spritesheet output (default 8 cols) |
| `--list`                                      | List available tilesets or sprites  |

### Flat output structure

```
out/
├── tilesets/<group>/{combined,bottom,top}.png
└── pokemon/sprites/<id>_<name>/{front,back}{,_shiny}.png
    └── forms/<form>/{front,back}.png
```

Symbol tables are bundled as TOML files in the binary. Override with `--sym-file` for ROM hacks.

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

If another tool (e.g. bgit) sets `core.hooksPath` globally, use `prek install --git-dir .git` and copy that tool's hooks into `.git/hooks/`. Alternatively, use `pre-commit` (pip).

### Publishing

Releases publish to crates.io and npm automatically when a version tag is pushed. Forks need `CRATES_IO_TOKEN` and `NPM_TOKEN` secrets.

```bash
# Bump version in Cargo.toml, commit, then:
git tag v0.4.0 && git push origin v0.4.0
```

## License

MIT OR Apache-2.0

## Attribution

The TOML memory maps are generated from [pret/pokeemerald](https://github.com/pret/pokeemerald/tree/symbols), [pret/pokefirered](https://github.com/pret/pokefirered/tree/symbols), and [pret/pokeruby](https://github.com/pret/pokeruby/tree/symbols).

This project contains no content from any Pokemon ROM dump.

See [Cargo.toml](Cargo.toml) for the full dependency list.
