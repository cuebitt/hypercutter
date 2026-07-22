//! Field effect palette resolution.
//!
//! The GBA Pokemon games use a bytecode-based scripting system for field
//! effects like tall grass, surf blobs, and shadow sprites. This module
//! walks those scripts and pulls out the 16-color palettes they reference.
//!
//! How it works:
//! - `gFieldEffectScriptPointers` (from the symbol table) is an array of
//!   pointers, one per field effect ID. Each pointer points to a little
//!   bytecode program.
//! - The bytecode has 8 opcodes. Opcodes 1, 2, 5, and 7 embed a pointer to a
//!   `SpritePalette` struct, which contains a tag and a pointer to 32 bytes
//!   of BGR555 palette data.
//! - We collect all palettes into a tag → palette map, then augment it with
//!   any `gSpritePalette_*` or `gFieldEffectObjectPaletteInfo*` entries from
//!   the symbol table.
//! - Callers look up a palette by reading the `paletteTag` from a
//!   `SpriteTemplate` struct and querying the map.

use std::collections::BTreeMap;

use crate::graphics::Rgba;
use crate::tileset::{read_ptr_table, Palette, PALETTE_COLORS};
use crate::{Rom, SymbolTable};

const MAX_SCRIPT_OPS: usize = 32;
const SPRITE_PALETTE_SIZE: usize = 8;
const PALETTE_DATA_BYTES: usize = PALETTE_COLORS * 2;

const OP_LOADTILES: u8 = 0;
const OP_LOADFADEDPAL: u8 = 1;
const OP_LOADPAL: u8 = 2;
const OP_CALLNATIVE: u8 = 3;
const OP_END: u8 = 4;
const OP_LOADGFX_CALLNATIVE: u8 = 5;
const OP_LOADTILES_CALLNATIVE: u8 = 6;
const OP_LOADFADEDPAL_CALLNATIVE: u8 = 7;

/// Build a tag→palette map from field effect scripts and symbol table entries.
///
/// Two sources, priority order:
/// 1. Field effect scripts read from the ROM
/// 2. `gSpritePalette_*` / `gFieldEffectObjectPaletteInfo*` entries in the
///    symbol table
pub fn build_palette_map(rom: &Rom, symbols: &SymbolTable) -> BTreeMap<u16, [Rgba; 16]> {
    let mut palettes = parse_field_effect_palettes(rom, symbols);
    for sym in symbols.iter() {
        if !sym.name.starts_with("gSpritePalette_")
            && !sym.name.starts_with("gFieldEffectObjectPaletteInfo")
        {
            continue;
        }
        if let Some((tag, pal)) = read_sprite_palette(rom, sym.address) {
            palettes.entry(tag).or_insert(pal);
        }
    }
    palettes
}

/// Walk every field effect script in the ROM and collect palettes by tag.
///
/// Returns an empty map if `gFieldEffectScriptPointers` isn't in the symbol
/// table or can't be read.
fn parse_field_effect_palettes(rom: &Rom, symbols: &SymbolTable) -> BTreeMap<u16, [Rgba; 16]> {
    let sym = match symbols.get("gFieldEffectScriptPointers") {
        Some(s) => s,
        None => return BTreeMap::new(),
    };
    let count = (sym.length / 4).min(256) as usize;
    if count == 0 {
        return BTreeMap::new();
    }
    let ptrs = match read_ptr_table(rom, sym.address, count) {
        Ok(p) => p,
        Err(_) => return BTreeMap::new(),
    };
    let mut palettes = BTreeMap::new();
    for &script_ptr in &ptrs {
        if let Some((tag, pal)) = parse_script_for_palette(rom, script_ptr) {
            palettes.entry(tag).or_insert(pal);
        }
    }
    palettes
}

/// Walk one field effect bytecode program and grab the first palette it loads.
///
/// Returns `None` if the script doesn't reference a palette or something goes
/// wrong reading the ROM.
fn parse_script_for_palette(rom: &Rom, script_ptr: u32) -> Option<(u16, [Rgba; 16])> {
    let base = rom.offset_of(script_ptr).ok()?;
    let bytes = rom.bytes();
    let mut pos = base;
    for _ in 0..MAX_SCRIPT_OPS {
        if pos >= bytes.len() {
            return None;
        }
        let op = bytes[pos];
        pos += 1;
        match op {
            OP_LOADFADEDPAL | OP_LOADPAL => {
                if pos + 4 > bytes.len() {
                    return None;
                }
                let pal_ptr = u32::from_le_bytes([
                    bytes[pos],
                    bytes[pos + 1],
                    bytes[pos + 2],
                    bytes[pos + 3],
                ]);
                return read_sprite_palette(rom, pal_ptr);
            }
            OP_LOADFADEDPAL_CALLNATIVE => {
                if pos + 8 > bytes.len() {
                    return None;
                }
                let pal_ptr = u32::from_le_bytes([
                    bytes[pos],
                    bytes[pos + 1],
                    bytes[pos + 2],
                    bytes[pos + 3],
                ]);
                return read_sprite_palette(rom, pal_ptr);
            }
            OP_LOADGFX_CALLNATIVE => {
                if pos + 12 > bytes.len() {
                    return None;
                }
                let pal_ptr = u32::from_le_bytes([
                    bytes[pos + 4],
                    bytes[pos + 5],
                    bytes[pos + 6],
                    bytes[pos + 7],
                ]);
                if let Some(pal) = read_sprite_palette(rom, pal_ptr) {
                    return Some(pal);
                }
                pos += 12;
            }
            OP_LOADTILES => {
                if pos + 4 > bytes.len() {
                    return None;
                }
                pos += 4;
            }
            OP_CALLNATIVE => {
                if pos + 4 > bytes.len() {
                    return None;
                }
                pos += 4;
            }
            OP_LOADTILES_CALLNATIVE => {
                if pos + 8 > bytes.len() {
                    return None;
                }
                pos += 8;
            }
            OP_END => return None,
            _ => return None,
        }
    }
    None
}

/// Read a `SpritePalette` struct from the ROM and decode its 16-color palette.
///
/// The struct is 8 bytes: `{ data_ptr: u32 LE, tag: u16 LE, padding: u16 }`.
/// `data_ptr` points to 32 bytes of BGR555 color data.
pub fn read_sprite_palette(rom: &Rom, pal_ptr: u32) -> Option<(u16, [Rgba; 16])> {
    let offset = rom.offset_of(pal_ptr).ok()?;
    let bytes = rom.bytes();
    if offset + SPRITE_PALETTE_SIZE > bytes.len() {
        return None;
    }
    let data_ptr = u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]);
    let tag = u16::from_le_bytes([bytes[offset + 4], bytes[offset + 5]]);
    let pal_offset = rom.offset_of(data_ptr).ok()?;
    if pal_offset + PALETTE_DATA_BYTES > bytes.len() {
        return None;
    }
    let pal = Palette::from_bgr555(&bytes[pal_offset..pal_offset + PALETTE_DATA_BYTES]).ok()?;
    Some((tag, *pal.as_array()))
}

/// Read the `paletteTag` field out of a `SpriteTemplate` struct.
///
/// The tag lives at offset 2 as a `u16` LE.
pub fn read_template_palette_tag(rom: &Rom, template_addr: u32) -> Option<u16> {
    let offset = rom.offset_of(template_addr).ok()?;
    let bytes = rom.bytes();
    if offset + 4 > bytes.len() {
        return None;
    }
    Some(u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]))
}
