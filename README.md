# hypercutter

Binary extraction tool for GBA Pokemon ROMs. Rust library + CLI + WebAssembly.

## Features

- Extract tiles, palettes, and metatiles from GBA Pokemon ROMs
- Supports Pokemon Emerald, FireRed, LeafGreen, Ruby, Sapphire
- Decompress LZSS-compressed tileset and sprite data
- Render metatiles as PNG images
- Dump Pokemon battle sprites (front, back, normal, shiny, alternate forms)
- Per-form palette and shiny palette support
- Extract overworld object event sprites (NPCs, Pokemon, items, decorations)
- Extract Pokemon footprints as PNG images
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

- `-g, --game`: Game to use for symbol file download (`emerald`, `firered`, `leafgreen`, `ruby`, `sapphire`)
- `--sym-file`: Path to a `.sym` file (auto-downloaded if not provided)
- `-e, --export`: Directory to export PNGs (default: `out`)
- `-v, --verbose`: Verbose output
- `-q, --quiet`: Suppress all non-error output
- `-c, --clear`: Clear export directory before writing
- `-y, --yes`: Clear without prompting
- `--overwrite`: Write over existing files without prompting (skips clear)

Output selection (mutually inclusive with each other; omitting all dumps everything):

- `--tilesets`: Render and export tileset PNGs
- `--sprites`: Dump Pokemon battle sprites
- `--overworld`: Dump overworld object event sprites

Output filtering:

- `--tileset-filter <PATTERN>`: Glob pattern to filter which tilesets to extract (e.g. `Battle*`)
- `--sprite-filter <PATTERN>`: Glob pattern to filter which sprites to extract (e.g. `pikachu`)

Sprite output options (only meaningful with `--sprites`):

- `--spritesheet`: Output sprites as a spritesheet instead of individual PNGs
- `--spritesheet-columns`: Columns in spritesheet (default: 8)

Overworld output options (only meaningful with `--overworld`):

- `--overworld-filter <PATTERN>`: Glob pattern to filter overworld sprites
- `--overworld-shiny`: Include shiny palette variants for overworld sprites
- `--overworld-spritesheet`: Output overworld sprites as a spritesheet
- `--overworld-columns`: Columns in overworld spritesheet (default: 8)

Other:

- `--list`: List available tilesets, sprites, or overworld objects without extracting

### Output directory structure

```
out/
├── tilesets/              # One PNG per metatile group
│   ├── General.png
│   ├── Petalburg.png
│   └── ...
├── sprites/pokemon/       # One subdirectory per species
│   ├── 001_bulbasaur/
│   │   ├── front.png
│   │   ├── front_shiny.png
│   │   ├── back.png
│   │   ├── back_shiny.png
│   │   └── footprint.png
│   ├── 025_pikachu/
│   │   └── ...
│   ├── egg/               # Egg sprite (species 412)
│   └── 000_??????????/    # Circled Question Mark (species 0)
├── sprites/overworld/     # One subdirectory per overworld sprite
│   ├── 000_Brendan/
│   │   ├── frame_0.png
│   │   └── ...
│   └── ...
└── forms/                 # Alternate forms (adjacent to sprites/pokemon)
    ├── unown_a/
    │   ├── front.png
    │   └── back.png
    └── ...
```

### Symbol file auto-download

The `.sym` file is auto-downloaded from the [pret](https://github.com/pret) disassembly
repositories (via jsDelivr CDN) and cached in your platform's standard cache directory.
Override with the `HYPERCUTTER_CACHE_DIR` environment variable.

## Library

```rust
use hypercutter::{
    Extractor, Footprint, FormSprite, Game, Rom, SpeciesId, Sprite, SymbolTable,
    TilesetRenderer, SpriteRenderer, render_footprint,
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

// Base species sprites (includes front, back, palette, footprint)
let sprites: Vec<Sprite> = extractor.sprites()?;
let names = extractor.species_names()?;
let national_dex = extractor.national_dex_map()?;

// Render a footprint to PNG
if let Some(ref fp) = sprites[0].footprint {
    let img = render_footprint(fp);
    // img.save_png("bulbasaur_footprint.png")?;
}

// Alternate forms
let forms: Vec<FormSprite> = extractor.forms()?;

// Overworld sprites (NPCs, Pokemon, items, decorations)
let overworld: Vec<OverworldSprite> = extractor.overworld_sprites()?;
```

## WebAssembly / JavaScript

Build the WASM package with [`wasm-pack`](https://github.com/rustwasm/wasm-pack):

```bash
cargo install wasm-pack
wasm-pack build --release
```

WASM optimization settings are configured in `Cargo.toml` under `[package.metadata.wasm-pack.profile.release]`. This produces a `pkg/` directory with
JS glue code, TypeScript types, and a `package.json` ready for npm publishing.

Once published to npm, install with:

```bash
pnpm install hypercutter
```

```js
import init, {
  HypercutterExtractor,
  identify_game,
  count_sym,
} from "hypercutter";

await init();

// Load ROM and sym file
const romBytes = new Uint8Array(
  await (await fetch("pokeemerald.gba")).arrayBuffer(),
);
const symText = await (await fetch("pokeemerald.sym")).text();

// Identify the game
console.log(identify_game(romBytes)); // "emerald"

// Count symbols in a .sym file
console.log(count_sym(symText)); // 14026

// Create an extractor
const ex = new HypercutterExtractor(romBytes, symText);
console.log(ex.game); // "emerald"

// List metatile groups
const names = ex.metatile_names();
console.log(names); // ["General", "Petalburg", ...]

// Render a tileset to PNG bytes
const pngBytes = ex.render_tileset("General");
const blob = new Blob([pngBytes], { type: "image/png" });
const url = URL.createObjectURL(blob);
// <img src={url} /> or save to disk

// List symbol names used by hypercutter (filtered subset of the memory map)
const symbols = ex.symbol_names();
console.log(symbols); // ["Start", "gMonFrontPicTable", "gTileset_Overworld", ...]

// List species names (by species index)
const species = ex.species_names();
console.log(species); // ["bulbasaur", "ivysaur", ...]

// Render a Pokémon sprite (by national dex ID) to PNG bytes
const spritePng = ex.render_sprite(1); // Bulbasaur front sprite

// List overworld sprite names
const overworld = ex.overworld_sprite_names();
console.log(overworld); // ["Brendan", "May", "CuttableTree", ...]

// Render a single frame of an overworld sprite to PNG bytes
const framePng = ex.render_overworld_frame("Brendan", 0);
```

### Vite

Vite does not support the WebAssembly ESM integration proposal used by `wasm-pack`
output. You need to install two dev dependencies:

```bash
pnpm add -D vite-plugin-wasm vite-plugin-top-level-await
```

Then add them to your `vite.config.ts`:

```ts
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";

export default defineConfig({
  plugins: [wasm(), topLevelAwait()],
});
```

### API reference

| JS name                                        | Type                                               | Description                                     |
| ---------------------------------------------- | -------------------------------------------------- | ----------------------------------------------- |
| `identify_game(romBytes)`                      | `(Uint8Array) => string`                           | Identify game from raw ROM bytes                |
| `count_sym(symText)`                           | `(string) => number`                               | Count symbols in a `.sym` file                  |
| `HypercutterExtractor` constructor             | `new (Uint8Array, string) => HypercutterExtractor` | Create an extractor from ROM bytes and sym text |
| `HypercutterExtractor.game`                    | `getter => string`                                 | Identified game short name                      |
| `HypercutterExtractor.metatile_names()`        | `() => string[]`                                   | Available metatile group names                  |
| `HypercutterExtractor.render_tileset(name)`    | `(string) => Uint8Array`                           | Render a tileset as PNG bytes                   |
| `HypercutterExtractor.species_names()`         | `() => string[]`                                   | All species names by species index order        |
| `HypercutterExtractor.symbol_names()`          | `() => string[]`                                   | Symbol names used by the extraction logic       |
| `HypercutterExtractor.render_sprite(id)`       | `(number) => Uint8Array`                           | Render a Pokémon front sprite as PNG bytes      |
| `HypercutterExtractor.overworld_sprite_names()`| `() => string[]`                                   | Names of all overworld object event sprites     |
| `HypercutterExtractor.render_overworld_frame(name, frame)` | `(string, number) => Uint8Array`        | Render a single overworld sprite frame          |

## Development

```bash
# Run tests (requires real ROMs in tests/fixtures/)
cargo test --all

# Lint
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked

# Verify crates.io packaging
cargo package --list
cargo publish --dry-run

# Build for WebAssembly
cargo build --release --target wasm32-unknown-unknown
```

### Pre-commit hooks

This repository uses pre-commit hooks for `cargo fmt` and `cargo clippy`, configured in
`.pre-commit-config.yaml`. [prek](https://github.com/j178/prek) is the preferred tool:

```bash
# Install prek (preferred, Rust-native)
cargo install prek --locked
prek install

# Run hooks manually
prek run --all-files
```

If another tool (e.g. bgit) has set `core.hooksPath` globally, `prek install` will refuse
to run. Use `--git-dir` to bypass the check and install into `.git/hooks/` directly:

```bash
prek install --git-dir .git
```

Note: this writes prek's shims to `.git/hooks/` while Git still uses the global
`core.hooksPath`. To make both work, copy or symlink bgit's hooks (e.g. `pre-push`) into
`.git/hooks/` as well.

Alternatively, use [pre-commit](https://pre-commit.com/):

```bash
pip install pre-commit
pre-commit install
pre-commit run --all-files
```

### Publishing

Releases are published automatically to crates.io and npm when a version tag (`v*`) is pushed.
The publish workflows are in `.github/workflows/`.

Forks need these repository secrets to publish:

| Secret            | Purpose              | Where to get it                                                    |
| ----------------- | -------------------- | ------------------------------------------------------------------ |
| `CRATES_IO_TOKEN` | Publish to crates.io | [crates.io/settings/tokens](https://crates.io/settings/tokens)     |
| `NPM_TOKEN`       | Publish to npm       | [npmjs.com → Access Tokens](https://www.npmjs.com/settings/tokens) |

To release:

```bash
# 1. Bump version in Cargo.toml
# 2. Commit and push
# 3. Tag and push the tag
git tag v0.4.0
git push origin v0.4.0
```

## License

MIT OR Apache-2.0

## Attribution

This project builds on the work of many open-source libraries and tools:

### Libraries

- [nintendo-lz](https://crates.io/crates/nintendo-lz): LZSS decompression
- [clap](https://crates.io/crates/clap): CLI argument parsing
- [ureq](https://crates.io/crates/ureq): HTTP client for symbol file downloads
- [png](https://crates.io/crates/png): PNG encoding for rendered images
- [binrw](https://crates.io/crates/binrw): Binary data parsing of ROM structures
- [bilge](https://crates.io/crates/bilge): Bitfield struct support
- [wasm-bindgen](https://crates.io/crates/wasm-bindgen) / [js-sys](https://crates.io/crates/js-sys): WebAssembly bindings
- [serde](https://crates.io/crates/serde): Serialization framework
- [thiserror](https://crates.io/crates/thiserror) / [anyhow](https://crates.io/crates/anyhow): Error handling
- [dialoguer](https://crates.io/crates/dialoguer): Interactive prompts
- [indicatif](https://crates.io/crates/indicatif): Progress bars
- [console](https://crates.io/crates/console): Terminal styling
- [rayon](https://crates.io/crates/rayon): Parallel iteration
- [glob](https://crates.io/crates/glob): Pattern matching
- [sha2](https://crates.io/crates/sha2): ROM hashing / game identification
- [dirs](https://crates.io/crates/dirs): Platform cache directory resolution

### Tools

- [wasm-pack](https://github.com/rustwasm/wasm-pack): WebAssembly build tooling

### Reference

- [pret/pokeemerald](https://github.com/pret/pokeemerald/tree/symbols), [pret/pokefirered](https://github.com/pret/pokefirered/tree/symbols), [pret/pokeruby](https://github.com/pret/pokeruby/tree/symbols)
  - Only the memory maps and symbol names are used. This project contains no content from any Pokemon ROM dump, no game code, and no copyrighted assets.
