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

// Render a Pokemon sprite (by national dex ID) to PNG bytes
const spritePng = ex.render_sprite(1); // Bulbasaur front sprite
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
