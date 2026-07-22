//! Extract tilesets, palettes, sprites, and field effects from GBA Pokemon
//! ROMs (Emerald, FireRed, LeafGreen, Ruby, Sapphire).
//!
//! # Quickstart
//!
//! ```no_run
//! use hypercutter::{Extractor, Rom, SymbolTable};
//!
//! let rom = Rom::open("pokeemerald.gba")?;
//! let symbols = SymbolTable::resolve_for_rom(&rom)?;
//! let extractor = Extractor::new(&rom, &symbols);
//! let metatiles = extractor.metatiles()?;
//! # Ok::<(), hypercutter::Error>(())
//! ```

#![doc(html_root_url = "https://docs.rs/hypercutter/")]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
mod error;
mod extract;
mod field_effect;
mod graphics;
mod lzss;
mod render;
mod rom;
mod sprite;
#[cfg(not(target_arch = "wasm32"))]
pub mod sprite_pack;
mod symbols;
mod tileset;

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(not(target_arch = "wasm32"))]
pub use cli::Cli;
pub use error::{Error, Result};
pub use extract::{pokemon_char, ExtractOptions, Extractor, MetatileEntry, Metatiles};
pub use graphics::{bgr555_to_rgba, decode_tile_4bpp, Rgba, RgbaImage};
pub use lzss::{decompress as decompress_lzss, is_lzss};
pub use render::{
    render_footprint, renderer_for_sprite, sprite_palette, SpriteRenderer, TilesetRenderer,
};
pub use rom::{Game, Rom, DEFAULT_ROM_BASE_ADDRESS, GAME_CODE_LENGTH, GAME_CODE_OFFSET};
pub use sprite::{
    Footprint, FormSprite, MonCoords, MonCoordsOnDisk, SpeciesId, Sprite, SpriteSheet,
    MON_PIC_HEIGHT_TILES, MON_PIC_PIXELS, MON_PIC_WIDTH_TILES, POKEMON_PALETTE_BYTES,
    POKEMON_PIC_BYTES,
};
pub use symbols::{Scope, Symbol, SymbolTable};
pub use tileset::{
    read_ptr_table, read_slice_at, read_struct_at, read_u32_at, MapLayout, Metatile, MetatileData,
    MetatileLayer, MetatileLayerBits, Palette, PaletteData, TileData, Tileset, TilesetHeader,
    MAP_LAYOUT_SIZE, METATILE_LAYER_COUNT, PALETTE_COLORS, PALETTE_COUNT, TILESET_SIZE, TILE_SIZE,
};
