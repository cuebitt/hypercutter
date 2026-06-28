//! WASM bindings for hypercutter.
//!
//! This is a skeleton: the exact JS surface will be finalized alongside the
//! rebuilt web app.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

use crate::{Extractor, Rom, SymbolTable};

/// Convert a hypercutter error to a JS string.
fn to_js<E: std::fmt::Display>(err: E) -> JsError {
    JsError::new(&err.to_string())
}

/// Symbol names referenced by the extraction logic (exact matches).
const USED_SYMBOL_NAMES: &[&str] = &[
    "Start",
    "gMapLayouts",
    "gMonBackPicCoords",
    "gMonBackPicTable",
    "gMonFrontPicCoords",
    "gMonFrontPicTable",
    "gMonPaletteTable",
    "gMonShinyPaletteTable",
    "gSpeciesNames",
    "sSpeciesToNationalPokedexNum",
];

/// Symbol name prefixes referenced by the extraction logic.
const USED_SYMBOL_PREFIXES: &[&str] = &[
    "gMetatiles_",
    "gMonBackPic_",
    "gMonFrontPic_",
    "gMonPalette_",
    "gMonShinyPalette_",
    "gTilesetPalettes_",
    "gTilesetTiles_",
    "gTileset_",
];

/// WASM-facing wrapper around an extraction session.
#[wasm_bindgen]
#[derive(Debug)]
pub struct HypercutterExtractor {
    rom: Rom,
    symbols: SymbolTable,
}

#[wasm_bindgen]
impl HypercutterExtractor {
    /// # Errors
    ///
    /// Returns a [`JsError`] if the ROM is invalid.
    #[wasm_bindgen(constructor)]
    pub fn new(rom_bytes: Vec<u8>, sym_text: &str) -> std::result::Result<Self, JsError> {
        let rom = Rom::from_bytes(rom_bytes).map_err(to_js)?;
        let symbols = SymbolTable::from_text(sym_text).map_err(to_js)?;
        Ok(Self { rom, symbols })
    }

    /// Returns the identified game's short name (or empty if unknown).
    #[wasm_bindgen(getter)]
    pub fn game(&self) -> String {
        self.rom.game().short_name().to_owned()
    }

    /// Returns the list of metatile names.
    #[wasm_bindgen]
    pub fn metatile_names(&self) -> std::result::Result<Vec<JsValue>, JsError> {
        let extractor = Extractor::new(&self.rom, &self.symbols);
        let metatiles = extractor.metatiles().map_err(to_js)?;
        Ok(metatiles.names().map(JsValue::from).collect())
    }

    /// Returns the RGBA bytes for a single tileset PNG.
    #[wasm_bindgen]
    pub fn render_tileset(&self, name: &str) -> std::result::Result<Vec<u8>, JsError> {
        use crate::TilesetRenderer;
        let extractor = Extractor::new(&self.rom, &self.symbols);
        let metatiles = extractor.metatiles().map_err(to_js)?;
        let Some(entry) = metatiles.get(name) else {
            return Err(JsError::new(&format!("unknown tileset: {name}")));
        };
        let renderer = match entry.secondary.as_ref() {
            Some(secondary) => TilesetRenderer::new(&entry.primary).with_secondary(secondary),
            None => TilesetRenderer::new(&entry.primary),
        };
        let mut png_bytes: Vec<u8> = Vec::new();
        let img = renderer.render();
        img.write_png(&mut png_bytes).map_err(to_js)?;
        Ok(png_bytes)
    }

    /// Returns the RGBA bytes for a single Pokemon sprite (front, normal).
    #[wasm_bindgen]
    pub fn render_sprite(&self, species_id: u16) -> std::result::Result<Vec<u8>, JsError> {
        use crate::SpriteRenderer;
        let extractor = Extractor::new(&self.rom, &self.symbols);
        let sprites = extractor.sprites().map_err(to_js)?;
        let Some(sprite) = sprites
            .iter()
            .find(|s| u32::from(s.id.0) == u32::from(species_id))
        else {
            return Err(JsError::new(&format!("unknown species: {species_id}")));
        };
        let Some(sheet) = sprite.front.as_ref() else {
            return Err(JsError::new("species has no front sprite"));
        };
        let Some(palette) = sprite.palette.get(0) else {
            return Err(JsError::new("species has no palette"));
        };
        let renderer = SpriteRenderer::new(sheet, palette);
        let mut png_bytes: Vec<u8> = Vec::new();
        renderer.render().write_png(&mut png_bytes).map_err(to_js)?;
        Ok(png_bytes)
    }

    /// Returns all species names (lowercased), ordered by species id.
    #[wasm_bindgen]
    pub fn species_names(&self) -> std::result::Result<Vec<JsValue>, JsError> {
        let extractor = Extractor::new(&self.rom, &self.symbols);
        let names = extractor.species_names().map_err(to_js)?;
        Ok(names.into_iter().map(JsValue::from).collect())
    }

    /// Returns the names of every symbol from the memory map that
    /// hypercutter references. Useful for clients that cache the
    /// symbols file so they can avoid storing irrelevant symbols.
    #[wasm_bindgen]
    pub fn symbol_names(&self) -> Vec<JsValue> {
        self.symbols
            .iter()
            .filter(|s| {
                USED_SYMBOL_NAMES.contains(&s.name.as_str())
                    || USED_SYMBOL_PREFIXES.iter().any(|p| s.name.starts_with(p))
            })
            .map(|s| JsValue::from(s.name.as_str()))
            .collect()
    }
}

/// Parse a `.sym` file from text. Returns the number of symbols.
#[wasm_bindgen]
#[allow(dead_code, unreachable_pub)] // exported via wasm_bindgen
pub fn count_sym(text: &str) -> std::result::Result<usize, JsError> {
    let table = SymbolTable::from_text(text).map_err(to_js)?;
    Ok(table.len())
}

/// Identify a game's short name from raw ROM bytes.
#[wasm_bindgen]
#[allow(dead_code, unreachable_pub)] // exported via wasm_bindgen
pub fn identify_game(rom_bytes: Vec<u8>) -> std::result::Result<String, JsError> {
    let rom = Rom::from_bytes(rom_bytes).map_err(to_js)?;
    Ok(rom.game().short_name().to_owned())
}
