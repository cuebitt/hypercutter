# WebAssembly / JavaScript

Build the WASM package with [wasm-pack](https://github.com/rustwasm/wasm-pack):

```bash
cargo install wasm-pack
wasm-pack build --release --target web       # general use / CI
wasm-pack build --release --target bundler   # npm publish
```

Either command produces a `pkg/` directory with JS glue code, TypeScript types, and a `package.json`.

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

// Load ROM
const romBytes = new Uint8Array(
  await (await fetch("pokeemerald.gba")).arrayBuffer(),
);

// Identify the game
console.log(identify_game(romBytes)); // "emerald"

// Create an extractor (uses bundled symbol tables)
const ex = HypercutterExtractor.with_bundled(romBytes);
// Or with a custom symbol file (TOML or .sym):
// const symText = await (await fetch("pokeemerald.toml")).text();
// const ex = new HypercutterExtractor(romBytes, symText);
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

// List species names
const species = ex.species_names();
console.log(species); // ["bulbasaur", "ivysaur", ...]

// Render a Pokemon sprite (by species ID) to PNG bytes
const spritePng = ex.render_sprite(1); // Bulbasaur front sprite
const backPng = ex.render_sprite_back(1); // Bulbasaur back sprite

// Render a Pokemon footprint to PNG bytes
const footprintPng = ex.render_footprint(1); // Bulbasaur footprint

// List alternate-form sprites
const altForms = ex.forms();
console.log(altForms); // [{base: 201, form: "B"}, {base: 386, form: "Attack"}, ...]

// Render an alternate-form sprite
const unownBPng = ex.render_form_sprite(201, "B");

// National Dex mapping
const dexMap = ex.national_dex_map();
console.log(dexMap[1]); // 1 (Bulbasaur = National Dex #1)
```

## Vite

Vite does not support the WebAssembly ESM integration proposal used by wasm-pack output. Install two dev dependencies:

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

## API reference

| JS name                                         | Type                                               | Description                                                         |
| ----------------------------------------------- | -------------------------------------------------- | ------------------------------------------------------------------- |
| `identify_game(romBytes)`                       | `(Uint8Array) => string`                           | Identify game from raw ROM bytes                                    |
| `count_sym(symText)`                            | `(string) => number`                               | Count symbols in a TOML or `.sym` file                              |
| `HypercutterExtractor` constructor              | `new (Uint8Array, string) => HypercutterExtractor` | Create an extractor from ROM bytes and symbol text (TOML or `.sym`) |
| `HypercutterExtractor.with_bundled(romBytes)`   | `(Uint8Array) => HypercutterExtractor`             | Create an extractor using the bundled symbol table                  |
| `HypercutterExtractor.game`                     | `getter => string`                                 | Identified game short name                                          |
| `HypercutterExtractor.metatile_names()`         | `() => string[]`                                   | Available metatile group names                                      |
| `HypercutterExtractor.render_tileset(name)`     | `(string) => Uint8Array`                           | Render a tileset as PNG bytes                                       |
| `HypercutterExtractor.species_names()`          | `() => string[]`                                   | All species names by dex order                                      |
| `HypercutterExtractor.symbol_names()`           | `() => string[]`                                   | Symbol names used by the extraction logic                           |
| `HypercutterExtractor.render_sprite(speciesId)` | `(number) => Uint8Array`                           | Render a Pokemon front sprite as PNG bytes                          |
| `HypercutterExtractor.render_sprite_back(speciesId)` | `(number) => Uint8Array`                     | Render a Pokemon back sprite as PNG bytes                           |
| `HypercutterExtractor.render_footprint(speciesId)` | `(number) => Uint8Array`                     | Render a Pokemon footprint as PNG bytes                             |
| `HypercutterExtractor.forms()`                  | `() => {base: number, form: string}[]`           | List alternate-form sprites                                         |
| `HypercutterExtractor.render_form_sprite(baseSpeciesId, form)` | `(number, string) => Uint8Array`     | Render an alternate-form front sprite as PNG bytes                  |
| `HypercutterExtractor.national_dex_map()`       | `() => Uint16Array`                                | National Dex mapping (index = species ID, value = dex number)       |
