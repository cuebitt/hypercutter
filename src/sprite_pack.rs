//! Sprite and field effect extraction.
//!
//! Two main jobs:
//! - Overworld character sprites from `gObjectEventGraphicsInfoPointers`,
//!   packed into RGBA PNGs with a `manifest.json`.
//! - Field effect sprites (tall grass, surf blob, shadows, etc.) from
//!   `gFieldEffectObjectPic_*` / `gFieldEffectPic_*` symbols, with palettes
//!   resolved from the field effect script bytecode.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use binrw::BinRead;
use console::style;

use log::warn;

use crate::field_effect::{build_palette_map as build_fe_palette_map, read_template_palette_tag};
use crate::graphics::{bgr555_to_rgba, decode_tile_4bpp, Rgba, RgbaImage};
use crate::lzss::{decompress as decompress_lzss, is_lzss};
use crate::tileset::read_struct_at;
use crate::{Game, Rom, SymbolTable};

/// Size of a single `SpriteFrameImage` struct in bytes.
const SPRITE_FRAME_IMAGE_SIZE: usize = 8;
/// Size of a single `AnimCmd` union in bytes.
const ANIM_CMD_SIZE: usize = 4;
/// Size of a GBA palette (16 colors * 2 bytes each).
const PALETTE_BYTES: usize = 32;
/// GBA ROM address window (cartridge is mapped here).
const GBA_LO: u32 = 0x0800_0000;
const GBA_HI: u32 = 0x0A00_0000;

// ---------------------------------------------------------------------------
// ROM struct definitions
// ---------------------------------------------------------------------------

/// On-disk `ObjectEventGraphicsInfo` (0x24 bytes).
#[derive(Debug, BinRead)]
#[br(little)]
#[allow(dead_code)]
struct RawObjectEventGraphicsInfo {
    tile_tag: u16,
    palette_tag: u16,
    reflection_palette_tag: u16,
    size: u16,
    width: i16,
    height: i16,
    palette_slot_bits: u8,
    tracks: u8,
    _pad: [u8; 2],
    oam_ptr: u32,
    subsprite_table_ptr: u32,
    anims_ptr: u32,
    images_ptr: u32,
    affine_anims_ptr: u32,
}

/// On-disk `SpriteFrameImage` (8 bytes).
#[derive(Debug, Clone, Copy, BinRead)]
#[br(little)]
#[allow(dead_code)]
struct RawSpriteFrameImage {
    data_ptr: u32,
    size: u16,
    #[br(pad_size_to = 2)]
    _pad: (),
}

/// On-disk `union AnimCmd` (4 bytes).
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum AnimCmd {
    Frame {
        image_value: u16,
        duration: u8,
        h_flip: bool,
        v_flip: bool,
    },
    Loop {
        count: u8,
    },
    Jump {
        target: u8,
    },
    End,
}

fn parse_anim_cmd(bytes: &[u8]) -> AnimCmd {
    let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let type_disc = i16::from_le_bytes([bytes[0], bytes[1]]);
    match type_disc {
        -1 => AnimCmd::End,
        -2 => AnimCmd::Jump {
            target: ((raw >> 16) & 0x3F) as u8,
        },
        -3 => AnimCmd::Loop {
            count: ((raw >> 16) & 0x3F) as u8,
        },
        _ => AnimCmd::Frame {
            image_value: (raw & 0xFFFF) as u16,
            duration: ((raw >> 16) & 0x3F) as u8,
            h_flip: (raw >> 22) & 1 != 0,
            v_flip: (raw >> 23) & 1 != 0,
        },
    }
}

// ---------------------------------------------------------------------------
// Palette resolution
// ---------------------------------------------------------------------------

/// Build a palette tag → `[Rgba; 16]` map once, then use for all sprites.
fn build_palette_map(rom: &Rom, symbols: &SymbolTable, game: Game) -> BTreeMap<u16, [Rgba; 16]> {
    if let Some(table) = symbols.get("sObjectEventSpritePalettes") {
        let map = read_palette_table(rom, table.address);
        if !map.is_empty() {
            return map;
        }
    }
    palette_candidate_fallback(rom, symbols, game)
}

/// Read the runtime sprite-palette lookup table from ROM.
///
/// Each entry is 8 bytes: `{ data_ptr: u32 LE, tag: u16 LE, padding: u16 }`.
///
/// Scans entries until a terminator (data_ptr=0 && tag=0) is found, with a
/// safety limit of 256 entries. Leading null entries (Ruby has 3 before real
/// data) are skipped.
fn read_palette_table(rom: &Rom, address: u32) -> BTreeMap<u16, [Rgba; 16]> {
    let mut map = BTreeMap::new();
    let rom_start = rom.base_address();
    let rom_end = rom_start + rom.bytes().len() as u32;
    for i in 0..256 {
        let entry_addr = address.wrapping_add((i * 8) as u32);
        let Ok(bytes) = rom.slice_at(entry_addr, 8) else {
            break;
        };
        let data_ptr = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let tag = u16::from_le_bytes([bytes[4], bytes[5]]);
        if data_ptr == 0 && tag == 0 {
            if map.is_empty() {
                continue;
            }
            break;
        }
        if data_ptr < rom_start || data_ptr >= rom_end {
            continue;
        }
        if let Ok(palette) = read_palette(rom, data_ptr) {
            map.insert(tag, palette);
        }
    }
    map
}

fn palette_candidate_fallback(
    rom: &Rom,
    symbols: &SymbolTable,
    game: Game,
) -> BTreeMap<u16, [Rgba; 16]> {
    use Game::*;
    let candidates: &[(u16, &str)] = match game {
        Emerald | Ruby | Sapphire => &[
            (0x1100, "gObjectEventPal_Brendan"),
            (0x1110, "gObjectEventPal_May"),
            (0x1103, "gObjectEventPal_Null1"),
            (0x1104, "gObjectEventPal_Null2"),
            (0x1105, "gObjectEventPal_Null3"),
            (0x1106, "gObjectEventPal_Null4"),
            (0x1107, "gObjectEventPal_Null1"),
            (0x110B, "gObjectEventPal_Brendan"),
            (0x1115, "gObjectEventPal_PlayerUnderwater"),
        ],
        FireRed | LeafGreen => &[
            (0x1100, "gObjectEventPal_Player"),
            (0x1110, "gObjectEventPal_Player"),
            (0x1103, "gObjectEventPal_NpcBlue"),
            (0x1104, "gObjectEventPal_NpcPink"),
            (0x1105, "gObjectEventPal_NpcGreen"),
            (0x1106, "gObjectEventPal_NpcWhite"),
            (0x110B, "gObjectEventPal_RSQuintyPlump"),
            (0x1113, "gObjectEventPal_Meteorite"),
            (0x1114, "gObjectEventPal_Seagallop"),
            (0x1115, "gObjectEventPal_SSAnne"),
        ],
    };
    let mut map = BTreeMap::new();
    for (tag, sym_name) in candidates {
        if let Some(sym) = symbols.get(sym_name) {
            if sym.length >= PALETTE_BYTES as u32 {
                if let Ok(palette) = read_palette(rom, sym.address) {
                    map.insert(*tag, palette);
                }
            }
        }
    }
    map
}

fn read_palette(rom: &Rom, address: u32) -> Result<[Rgba; 16]> {
    let data = rom
        .slice_at(address, PALETTE_BYTES)
        .with_context(|| format!("reading palette at 0x{address:08x}"))?;
    let mut colors = [Rgba::TRANSPARENT; 16];
    for (i, color) in colors.iter_mut().enumerate() {
        let lo = u16::from(data[i * 2]);
        let hi = u16::from(data[i * 2 + 1]);
        *color = bgr555_to_rgba((hi << 8) | lo);
    }
    Ok(colors)
}

// ---------------------------------------------------------------------------
// Sprite extraction
// ---------------------------------------------------------------------------

/// One extracted overworld sprite frame (full RGBA).
struct OverworldFrame {
    img: RgbaImage,
    h_flip: bool,
    v_flip: bool,
    image_value: u16,
}

/// All data needed to compose a facing_frames sheet for one sprite.
struct ExtractedOverworldSprite {
    name: String,
    frames: Vec<Vec<OverworldFrame>>,
}

/// Validate sprite dimensions and compute tile geometry.
/// Returns `(width, height, tile_w, tile_h, max_frames)` or `None` if the sprite should be skipped.
fn sprite_geometry(raw: &RawObjectEventGraphicsInfo) -> Option<(u32, u32, usize, usize, usize)> {
    if raw.width <= 0 || raw.height <= 0 {
        return None;
    }
    let w = raw.width as u32;
    let h = raw.height as u32;
    if w > 64 || h > 64 {
        return None;
    }
    let tile_w = (w / 8) as usize;
    let tile_h = (h / 8) as usize;
    let tiles_per_frame = tile_w * tile_h;
    let max_frames = if tiles_per_frame > 0 && raw.size > 0 {
        (raw.size as usize).div_ceil(tiles_per_frame)
    } else {
        0
    };
    if max_frames == 0 || raw.images_ptr == 0 {
        return None;
    }
    Some((w, h, tile_w, tile_h, max_frames))
}

/// Extract all overworld sprites from the ROM.
fn extract_overworld_sprites(
    rom: &Rom,
    symbols: &SymbolTable,
    palette_map: &BTreeMap<u16, [Rgba; 16]>,
) -> Result<Vec<ExtractedOverworldSprite>> {
    let table_sym = symbols
        .get("gObjectEventGraphicsInfoPointers")
        .ok_or_else(|| anyhow::anyhow!("symbol gObjectEventGraphicsInfoPointers not found"))?;

    let entry_count = (table_sym.length as usize) / 4;
    if entry_count == 0 {
        return Ok(Vec::new());
    }

    let ptrs = crate::tileset::read_ptr_table(rom, table_sym.address, entry_count)?;
    let mut sprites = Vec::with_capacity(entry_count);

    // Build reverse address → symbol name map.
    let addr_to_name: BTreeMap<u32, &str> = symbols
        .iter()
        .filter_map(|sym| {
            if sym.name.starts_with("gObjectEventGraphicsInfo_") {
                Some((sym.address, sym.name.as_str()))
            } else {
                None
            }
        })
        .collect();

    // Default transparent palette as fallback.
    let default_palette = [Rgba::TRANSPARENT; 16];

    let rom_start = rom.base_address();
    let rom_end = rom_start + rom.bytes().len() as u32;

    // Pre-scan: build sorted list of sprites by images_ptr to detect image
    // table overlaps. When a frame's entry address is closer to another
    // sprite's images_ptr, that sprite owns the entry.
    let mut sprites_by_images_ptr: Vec<(u32, u32, usize)> = Vec::new(); // (images_ptr, sprite_ptr, max_frames)
    for &ptr in &ptrs {
        if ptr == 0 || ptr < rom_start || ptr >= rom_end {
            continue;
        }
        if !addr_to_name.contains_key(&ptr) {
            continue;
        }
        let Ok(raw) = read_struct_at::<RawObjectEventGraphicsInfo>(rom, ptr) else {
            continue;
        };
        let Some((_w, _h, _tile_w, _tile_h, max_frames)) = sprite_geometry(&raw) else {
            continue;
        };
        sprites_by_images_ptr.push((raw.images_ptr, ptr, max_frames));
    }
    sprites_by_images_ptr.sort_by_key(|&(ip, _, _)| ip);

    for &ptr in &ptrs {
        // Skip null pointers (unused table slots).
        if ptr == 0 || ptr < rom_start || ptr >= rom_end {
            continue;
        }

        let name = addr_to_name
            .get(&ptr)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("entry_{ptr:08x}"));

        let raw: RawObjectEventGraphicsInfo = read_struct_at(rom, ptr)
            .with_context(|| format!("reading ObjectEventGraphicsInfo for {name}"))?;

        let Some((_w, _h, tile_w, tile_h, max_frames)) = sprite_geometry(&raw) else {
            continue;
        };

        let palette = palette_map
            .get(&raw.palette_tag)
            .copied()
            .unwrap_or(default_palette);

        // Read animation direction pointers.
        let anim_dir_ptrs = read_anim_direction_ptrs(rom, raw.anims_ptr, max_frames);

        if anim_dir_ptrs.is_empty() || raw.images_ptr == 0 {
            continue;
        }

        // Use walking directions (indices 4-7) when available, otherwise
        // fall back to standing directions (indices 0-3).
        let walking_ptrs: Vec<u32> = if anim_dir_ptrs.len() >= 8 {
            anim_dir_ptrs[4..8].to_vec()
        } else if anim_dir_ptrs.len() >= 4 {
            anim_dir_ptrs[0..4].to_vec()
        } else {
            anim_dir_ptrs.clone()
        };

        let mut direction_frames: Vec<Vec<OverworldFrame>> = Vec::new();

        for &dir_ptr in walking_ptrs.iter() {
            if dir_ptr == 0 {
                continue;
            }
            let cmds = read_anim_cmds(rom, dir_ptr)?;
            let mut frames_for_dir = Vec::new();

            for cmd in &cmds {
                if let AnimCmd::Frame {
                    image_value,
                    h_flip,
                    v_flip,
                    ..
                } = cmd
                {
                    let img_idx = *image_value as usize;
                    if max_frames != 0 && img_idx >= max_frames {
                        continue;
                    }
                    let img_entry_addr = raw
                        .images_ptr
                        .wrapping_add((img_idx * SPRITE_FRAME_IMAGE_SIZE) as u32);
                    // Check if this entry is owned by another sprite — when image
                    // tables overlap, a closer images_ptr means that sprite owns
                    // the entry and the tile data belongs to it, not us.
                    let is_foreign = {
                        let idx = sprites_by_images_ptr
                            .partition_point(|&(ip, _, _)| ip <= img_entry_addr);
                        if idx > 0 {
                            let &(ip, other_ptr, mf) = &sprites_by_images_ptr[idx - 1];
                            ip > raw.images_ptr
                                && other_ptr != ptr
                                && img_entry_addr
                                    < ip.wrapping_add((mf * SPRITE_FRAME_IMAGE_SIZE) as u32)
                        } else {
                            false
                        }
                    };
                    if is_foreign {
                        continue;
                    }
                    let Ok(img_entry): Result<RawSpriteFrameImage, _> =
                        read_struct_at(rom, img_entry_addr)
                    else {
                        warn!("sprite {name} at 0x{ptr:08x}: failed to read frame entry at 0x{img_entry_addr:08x}");
                        continue;
                    };

                    if img_entry.data_ptr == 0 || img_entry.size == 0 {
                        continue;
                    }

                    let Ok(tile_data) =
                        read_sprite_tile_data(rom, img_entry.data_ptr, img_entry.size as usize)
                    else {
                        warn!("sprite {name} at 0x{ptr:08x}: failed to read tile data at 0x{:08x} (size {})", img_entry.data_ptr, img_entry.size);
                        continue;
                    };

                    let frame_img = render_overworld_frame(&tile_data, tile_w, tile_h, &palette);

                    frames_for_dir.push(OverworldFrame {
                        img: frame_img,
                        h_flip: *h_flip,
                        v_flip: *v_flip,
                        image_value: *image_value,
                    });
                }
            }

            if !frames_for_dir.is_empty() {
                // Deduplicate frames by image_value (keep first occurrence order).
                let mut seen_values = HashSet::new();
                frames_for_dir.retain(|f| seen_values.insert(f.image_value));
                direction_frames.push(frames_for_dir);
            }
        }

        if direction_frames.is_empty() {
            continue;
        }

        while direction_frames.len() < 4 {
            direction_frames.push(Vec::new());
        }

        sprites.push(ExtractedOverworldSprite {
            name,
            frames: direction_frames,
        });
    }

    Ok(sprites)
}

/// Read the animation direction pointers from `anims_ptr`.
/// Reads up to 8 entries: first 4 are standing (FACE), next 4 are walking (GO).
/// Returns all valid pointers found.
fn read_anim_direction_ptrs(rom: &Rom, anims_ptr: u32, max_frames: usize) -> Vec<u32> {
    if anims_ptr == 0 {
        return Vec::new();
    }
    let mut ptrs = Vec::new();
    // Read up to 8 to cover both FACE (0-3) and GO (4-7) direction tables.
    for i in 0..8 {
        let addr = anims_ptr.wrapping_add((i * 4) as u32);
        let ptr = match crate::tileset::read_u32_at(rom, addr) {
            Ok(p) => p,
            Err(_) => break,
        };
        if ptr == 0 || !(GBA_LO..GBA_HI).contains(&ptr) {
            break;
        }
        if !direction_is_valid(rom, ptr, max_frames) {
            break;
        }
        ptrs.push(ptr);
    }
    ptrs
}

/// Read animation commands from a direction's command array.
fn read_anim_cmds(rom: &Rom, cmd_ptr: u32) -> Result<Vec<AnimCmd>> {
    let mut cmds = Vec::new();
    let mut addr = cmd_ptr;
    loop {
        // Safety limit: no more than 256 commands per direction.
        if cmds.len() > 255 {
            break;
        }
        let bytes = rom.slice_at(addr, ANIM_CMD_SIZE)?;
        let cmd = parse_anim_cmd(bytes);
        match cmd {
            // Stop on END or JUMP (JUMP restarts the animation; we just collect
            // the frames in this single sequence).
            AnimCmd::End | AnimCmd::Jump { .. } => break,
            _ => cmds.push(cmd),
        }
        addr = addr.wrapping_add(ANIM_CMD_SIZE as u32);
    }
    Ok(cmds)
}

/// Validate that a direction command array contains only frames within
/// the sprite's maximum frame count. Returns `true` if the direction has
/// at least one valid frame and all frames' `image_value` < `max_frames`.
fn direction_is_valid(rom: &Rom, dir_ptr: u32, max_frames: usize) -> bool {
    let Ok(cmds) = read_anim_cmds(rom, dir_ptr) else {
        return false;
    };
    let mut has_frame = false;
    for cmd in &cmds {
        if let AnimCmd::Frame { image_value, .. } = cmd {
            has_frame = true;
            if max_frames > 0 && *image_value as usize >= max_frames {
                return false;
            }
        }
    }
    has_frame
}

/// Read sprite tile data, possibly LZSS-decompressing it.
fn read_sprite_tile_data(rom: &Rom, data_ptr: u32, reported_size: usize) -> Result<Vec<u8>> {
    let data = rom.slice_at(data_ptr, reported_size)?;
    if is_lzss(data) {
        decompress_lzss(data).map_err(|e| anyhow::anyhow!("LZSS decompress: {e}"))
    } else {
        Ok(data.to_vec())
    }
}

/// Render a single overworld frame from 4bpp tile data.
fn render_overworld_frame(
    tile_data: &[u8],
    tile_w: usize,
    tile_h: usize,
    palette: &[Rgba; 16],
) -> RgbaImage {
    let width_px = (tile_w * 8) as u32;
    let height_px = (tile_h * 8) as u32;
    let mut img = RgbaImage::new(width_px, height_px);

    for ty in 0..tile_h {
        for tx in 0..tile_w {
            let tile_idx = ty * tile_w + tx;
            let tile_start = tile_idx * 32;
            let tile_end = tile_start + 32;
            let tile = if tile_end <= tile_data.len() {
                &tile_data[tile_start..tile_end]
            } else {
                &[][..]
            };
            let indices = decode_tile_4bpp(tile);
            for (i, &idx) in indices.iter().enumerate() {
                let x = (tx * 8 + (i % 8)) as u32;
                let y = (ty * 8 + (i / 8)) as u32;
                let color = palette[idx as usize % 16];
                img.set_pixel(x, y, if idx == 0 { Rgba::TRANSPARENT } else { color });
            }
        }
    }
    img
}

/// Apply h_flip/v_flip to a frame image.
fn apply_flips(img: &RgbaImage, h_flip: bool, v_flip: bool) -> RgbaImage {
    if !h_flip && !v_flip {
        return img.clone();
    }
    let w = img.width();
    let h = img.height();
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let sx = if h_flip { w - 1 - x } else { x };
            let sy = if v_flip { h - 1 - y } else { y };
            if let Some(px) = img.pixel(sx, sy) {
                out.set_pixel(x, y, px);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Pack writer
// ---------------------------------------------------------------------------

/// Manifest entry for a single sprite.
#[derive(serde::Serialize, serde::Deserialize)]
struct ManifestSprite {
    file: String,
    layout: String,
    frames: u32,
    cell_size: u32,
}

/// Pack manifest.
#[derive(serde::Serialize, serde::Deserialize)]
struct Manifest {
    tile_size: u32,
    sprites: BTreeMap<String, ManifestSprite>,
    objects: BTreeMap<String, ManifestSprite>,
}

/// Category for an overworld sprite.
enum SpriteCategory {
    /// Character NPC sprite (trainers, townsfolk, etc.).
    CharacterSprite,
    /// Field object (dolls, rocks, signs, pokemon encounters, etc.).
    FieldObject,
}

/// Write a sprite pack to `dir`.
///
/// # Errors
///
/// Returns an error if reading from the ROM, writing files, or validating
/// the output fails.
pub fn write_pack(rom: &Rom, symbols: &SymbolTable, dir: &Path, quiet: bool) -> Result<()> {
    let palette_map = build_palette_map(rom, symbols, rom.game());
    let sprites = extract_overworld_sprites(rom, symbols, &palette_map)
        .context("extracting overworld sprites")?;

    if sprites.is_empty() {
        anyhow::bail!("no overworld sprites found in ROM");
    }

    let sprites_dir = dir.join("sprites");
    let objects_dir = dir.join("objects");
    std::fs::create_dir_all(&sprites_dir)
        .with_context(|| format!("creating {}", sprites_dir.display()))?;
    std::fs::create_dir_all(&objects_dir)
        .with_context(|| format!("creating {}", objects_dir.display()))?;

    let mut manifest_sprites: BTreeMap<String, ManifestSprite> = BTreeMap::new();
    let mut manifest_objects: BTreeMap<String, ManifestSprite> = BTreeMap::new();

    // Tile size in pixels for the square cell in facing_frames grids.
    // Gen 3 overworld sprites are 16×16 or 16×32; we pad to 32×32.
    let tile_size: u32 = 32;

    for sprite in &sprites {
        let category = classify_sprite(&sprite.name);

        // Frames per direction = max across all directions after dedup.
        let frames = sprite
            .frames
            .iter()
            .map(|dir| dir.len())
            .max()
            .unwrap_or(1)
            .max(1) as u32;

        if frames == 0 {
            continue;
        }

        // Use tile_size but expand to fit the largest frame dimension.
        let max_frame_dim = sprite
            .frames
            .iter()
            .flat_map(|dir| dir.iter())
            .map(|f| f.img.width().max(f.img.height()))
            .max()
            .unwrap_or(tile_size);
        let cell_px = tile_size.max(max_frame_dim);

        let sheet_w = cell_px * frames;
        let sheet_h = cell_px * 4;
        let mut sheet = RgbaImage::new(sheet_w, sheet_h);

        for (dir_idx, dir_frames) in sprite.frames.iter().enumerate() {
            for (frame_idx, frame) in dir_frames.iter().enumerate() {
                if frame_idx >= frames as usize {
                    break;
                }
                let cx = frame_idx as u32 * cell_px;
                let cy = dir_idx as u32 * cell_px;

                let flipped = apply_flips(&frame.img, frame.h_flip, frame.v_flip);

                let offset_x = (cell_px - flipped.width()) / 2;
                let offset_y = (cell_px - flipped.height()) / 2;
                sheet.alpha_blit(&flipped, (cx + offset_x, cy + offset_y));
            }
        }

        let sprite_id = derive_sprite_id(&sprite.name);

        let (folder, target_map) = match category {
            SpriteCategory::CharacterSprite => ("sprites", &mut manifest_sprites),
            SpriteCategory::FieldObject => ("objects", &mut manifest_objects),
        };

        let png_name = format!("{folder}/{sprite_id}.png");
        let png_path = dir.join(&png_name);
        sheet
            .save_png(&png_path)
            .with_context(|| format!("saving {}", png_path.display()))?;

        target_map.insert(
            sprite_id,
            ManifestSprite {
                file: png_name,
                layout: "facing_frames".to_string(),
                frames,
                cell_size: cell_px,
            },
        );
    }

    let sprite_count = manifest_sprites.len();
    let object_count = manifest_objects.len();

    let manifest = Manifest {
        tile_size,
        sprites: manifest_sprites,
        objects: manifest_objects,
    };
    let manifest_path = dir.join("manifest.json");
    let manifest_json =
        serde_json::to_string_pretty(&manifest).with_context(|| "serializing manifest")?;
    std::fs::write(&manifest_path, &manifest_json)
        .with_context(|| format!("writing {}", manifest_path.display()))?;

    validate_pack(dir, &manifest)?;

    if !quiet {
        println!("  Sprite pack written to {}", dir.display());
        println!("    tile_size: {tile_size}, sprites: {sprite_count}, objects: {object_count}");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Field effect sprite extraction
// ---------------------------------------------------------------------------

/// Field effect pic symbol prefixes we extract.
const FIELD_EFFECT_PIC_PREFIXES: &[&str] = &["gFieldEffectObjectPic_", "gFieldEffectPic_"];

/// Decode a raw GBA packed OAM entry (6 bytes) into pixel dimensions.
fn gba_oam_dims(oam_bytes: &[u8]) -> Option<(u32, u32)> {
    if oam_bytes.len() < 6 {
        return None;
    }
    let attr0 = u16::from_le_bytes([oam_bytes[0], oam_bytes[1]]);
    let attr1 = u16::from_le_bytes([oam_bytes[2], oam_bytes[3]]);
    let shape = (attr0 >> 14) & 3;
    let size = (attr1 >> 14) & 3;
    let dims = match (shape, size) {
        (0, 0) => (8, 8),
        (0, 1) => (16, 16),
        (0, 2) => (32, 32),
        (0, 3) => (64, 64),
        (1, 0) => (16, 8),
        (1, 1) => (32, 8),
        (1, 2) => (32, 16),
        (1, 3) => (64, 32),
        (2, 0) => (8, 16),
        (2, 1) => (8, 32),
        (2, 2) => (16, 32),
        (2, 3) => (32, 64),
        _ => return None,
    };
    Some(dims)
}

/// Figure out which palette a field effect sprite should use.
///
/// Tries `gFieldEffectObjectTemplate_*` first (Emerald/FireRed), then
/// `gFieldEffectSpriteTemplate_*` (Ruby/Sapphire). Falls back to the
/// grayscale `default_palette` if there's no template, the `paletteTag` is
/// 0xFFFF, or the tag isn't in the palette map.
fn resolve_palette_for_effect(
    rom: &Rom,
    symbols: &SymbolTable,
    palettes: &BTreeMap<u16, [Rgba; 16]>,
    default_palette: &[Rgba; 16],
    name: &str,
) -> [Rgba; 16] {
    for tmpl_name in [
        format!("gFieldEffectObjectTemplate_{name}"),
        format!("gFieldEffectSpriteTemplate_{name}"),
    ] {
        let sym = match symbols.get(&tmpl_name) {
            Some(s) => s,
            None => continue,
        };
        let tag = match read_template_palette_tag(rom, sym.address) {
            Some(t) => t,
            None => continue,
        };
        if tag == 0xFFFF {
            continue;
        }
        if let Some(&pal) = palettes.get(&tag) {
            return pal;
        }
    }
    *default_palette
}

/// Read a field effect's sprite template and return per-frame pixel dimensions.
///
/// Tries `gFieldEffectObjectTemplate_*` first (Emerald/FireRed), then
/// `gFieldEffectSpriteTemplate_*` (Ruby/Sapphire).
fn field_effect_frame_dims(rom: &Rom, symbols: &SymbolTable, name: &str) -> Option<(u32, u32)> {
    let tmpl_name = format!("gFieldEffectObjectTemplate_{name}");
    let sym = symbols
        .get(&tmpl_name)
        .or_else(|| symbols.get(&format!("gFieldEffectSpriteTemplate_{name}")))?;
    let oam_offset = sym.address.wrapping_add(4); // oamPtr is 4 bytes in
    let oam_ptr_bytes = rom.slice_at(oam_offset, 4).ok()?;
    let oam_ptr = u32::from_le_bytes([
        oam_ptr_bytes[0],
        oam_ptr_bytes[1],
        oam_ptr_bytes[2],
        oam_ptr_bytes[3],
    ]);
    let oam_data = rom.slice_at(oam_ptr, 6).ok()?;
    gba_oam_dims(oam_data)
}

/// Render a single frame of tile data into an RGBA image at the given size.
fn render_tile_grid(raw_tiles: &[u8], tile_w: u32, tile_h: u32, palette: &[Rgba; 16]) -> RgbaImage {
    let w = tile_w * 8;
    let h = tile_h * 8;
    let mut img = RgbaImage::new(w, h);
    for ty in 0..tile_h {
        for tx in 0..tile_w {
            let ti = (ty * tile_w + tx) as usize;
            let start = ti * 32;
            let chunk = if start + 32 <= raw_tiles.len() {
                &raw_tiles[start..start + 32]
            } else {
                continue;
            };
            let indices = decode_tile_4bpp(chunk);
            for (i, &idx) in indices.iter().enumerate() {
                let x = tx * 8 + (i as u32 % 8);
                let y = ty * 8 + (i as u32 / 8);
                let color = palette[idx as usize % 16];
                img.set_pixel(x, y, if idx == 0 { Rgba::TRANSPARENT } else { color });
            }
        }
    }
    img
}

/// Extract field effect sprites to `dir/field_effects/` as RGBA PNGs.
///
/// Palettes come from the field effect script bytecode. We walk
/// `gFieldEffectScriptPointers`, find every `SpritePalette` reference in the
/// scripts, and build a tag→palette map. Each sprite's palette is resolved
/// by reading the `paletteTag` from its `SpriteTemplate` struct and looking
/// up the tag in that map. Effects with `paletteTag = 0xFFFF` or no matching
/// template get a grayscale fallback.
///
/// If a `gFieldEffectObjectTemplate_*` (or `gFieldEffectSpriteTemplate_*`)
/// symbol exists, the OAM data determines per-frame dimensions. Multi-frame
/// animations are split into separate PNGs. Without a template, tiles are
/// laid out in a single horizontal strip.
///
/// # Errors
///
/// Returns an error if a PNG can't be written.
pub fn write_field_effects(
    rom: &Rom,
    symbols: &SymbolTable,
    dir: &Path,
    quiet: bool,
) -> Result<usize> {
    let fx_dir = dir.join("field_effects");
    std::fs::create_dir_all(&fx_dir).with_context(|| format!("creating {}", fx_dir.display()))?;

    let palettes = build_fe_palette_map(rom, symbols);

    let default_palette = {
        let mut p = [Rgba::TRANSPARENT; 16];
        for i in 1..16 {
            let v = i as u8 * 17;
            p[i as usize] = Rgba(v, v, v, 255);
        }
        p
    };

    let mut pics: Vec<(String, u32, usize)> = Vec::new();

    if let Some(sym) = symbols.get("gObjectEventPic_SurfBlob") {
        if sym.length > 0 {
            pics.push(("SurfBlob".to_owned(), sym.address, sym.length as usize));
        }
    }

    for sym in symbols.iter() {
        if sym.length == 0 {
            continue;
        }
        let short = FIELD_EFFECT_PIC_PREFIXES
            .iter()
            .find_map(|p| sym.name.strip_prefix(p))
            .map(|s| s.to_owned());
        if let Some(short) = short {
            pics.push((short, sym.address, sym.length as usize));
        }
    }

    if pics.is_empty() {
        return Ok(0);
    }

    if !quiet {
        println!(
            "  {} Extracting {} field effects...",
            style("\u{2192}").cyan().bold(),
            pics.len(),
        );
    }

    for (name, addr, byte_len) in &pics {
        let Some(offset) = rom.offset_of(*addr).ok() else {
            continue;
        };
        let end = (offset + byte_len).min(rom.bytes().len());
        let raw = &rom.bytes()[offset..end];
        let tile_count = raw.len() / 32;
        if tile_count == 0 {
            continue;
        }

        let palette = resolve_palette_for_effect(rom, symbols, &palettes, &default_palette, name);

        if let Some((fw, fh)) = field_effect_frame_dims(rom, symbols, name) {
            let fw_tiles = fw / 8;
            let fh_tiles = fh / 8;
            let tiles_per_frame = (fw_tiles * fh_tiles) as usize;
            if tiles_per_frame == 0 {
                continue;
            }
            let frame_count = tile_count.div_ceil(tiles_per_frame);
            let sheet_w = fw * frame_count as u32;
            let sheet_h = fh;
            let mut sheet = RgbaImage::new(sheet_w, sheet_h);
            for fi in 0..frame_count {
                let start = fi * tiles_per_frame * 32;
                let frame_end = (start + tiles_per_frame * 32).min(raw.len());
                let frame_data = &raw[start..frame_end];
                let img = render_tile_grid(frame_data, fw_tiles, fh_tiles, &palette);
                sheet.alpha_blit(&img, (fi as u32 * fw, 0));
            }
            let path = fx_dir.join(format!("{name}.png"));
            sheet
                .save_png(&path)
                .with_context(|| format!("saving {}", path.display()))?;
        } else {
            let width = tile_count as u32 * 8;
            let height = 8u32;
            let mut img = RgbaImage::new(width, height);
            for (ti, chunk) in raw.chunks(32).enumerate() {
                let indices = decode_tile_4bpp(chunk);
                for (i, &idx) in indices.iter().enumerate() {
                    let x = (ti as u32 * 8) + (i as u32 % 8);
                    let y = i as u32 / 8;
                    let color = palette[idx as usize % 16];
                    img.set_pixel(x, y, if idx == 0 { Rgba::TRANSPARENT } else { color });
                }
            }
            let path = fx_dir.join(format!("{name}.png"));
            img.save_png(&path)
                .with_context(|| format!("saving {}", path.display()))?;
        }
    }

    Ok(pics.len())
}

fn derive_sprite_id(full_name: &str) -> String {
    let stripped = full_name
        .strip_prefix("gObjectEventGraphicsInfo_")
        .unwrap_or(full_name);
    stripped.to_lowercase()
}

fn classify_sprite(name: &str) -> SpriteCategory {
    // Field objects — interactables, dolls, wild pokemon encounters.
    // Match on the stripped sprite ID to avoid catching RubySapphire* characters.
    let sprite_id = derive_sprite_id(name);
    let field_ids = [
        "itemball",
        "townmap",
        "pokedex",
        "fossil",
        "gymsign",
        "sign",
        "trainertips",
        "clipboard",
        "meteorite",
        "seagallop",
        "ssanne",
        "snorlax",
        "quintyplump",
        "ruby",
        "sapphire",
        "oldamber",
        "berrytree",
        "berrytreeearlystages",
        "berrytreelatestages",
        "cuttabletree",
        "breakablerock",
        "pushableboulder",
        "ballcushion",
    ];
    if field_ids.contains(&sprite_id.as_str()) {
        return SpriteCategory::FieldObject;
    }

    // Prefix-based field objects.
    if sprite_id.ends_with("doll") || sprite_id.starts_with("big") {
        return SpriteCategory::FieldObject;
    }

    // Pokemon encounter sprites (individual species).
    let pokemon_field = [
        "spearow",
        "cubone",
        "poliwrath",
        "clefairy",
        "pidgeot",
        "jigglypuff",
        "pidgey",
        "chansey",
        "omanyte",
        "kangaskhan",
        "pikachu",
        "psyduck",
        "nidoran",
        "nidorino",
        "meowth",
        "seel",
        "voltorb",
        "slowpoke",
        "slowbro",
        "machop",
        "wigglytuff",
        "doduo",
        "fearow",
        "machoke",
        "lapras",
        "zapdos",
        "moltres",
        "articuno",
        "mewtwo",
        "mew",
        "entei",
        "suicune",
        "raikou",
        "lugia",
        "hooh",
        "celebi",
        "kabuto",
        "deoxys",
    ];
    if pokemon_field.contains(&sprite_id.as_str()) {
        return SpriteCategory::FieldObject;
    }

    SpriteCategory::CharacterSprite
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn validate_pack(dir: &Path, manifest: &Manifest) -> Result<()> {
    for (category, entries) in [
        ("sprites", &manifest.sprites),
        ("objects", &manifest.objects),
    ] {
        for (id, sprite_def) in entries {
            let png_path = dir.join(&sprite_def.file);
            if !png_path.exists() {
                anyhow::bail!("missing PNG for {category} {id}: {}", png_path.display());
            }

            let file = std::fs::File::open(&png_path)
                .with_context(|| format!("opening {}", png_path.display()))?;
            let mut decoder = png::Decoder::new(file);
            let reader = decoder
                .read_header_info()
                .with_context(|| format!("reading PNG header for {id}"))?;
            let (w, h) = (reader.width, reader.height);

            match sprite_def.layout.as_str() {
                "facing_frames" => {
                    let cs = sprite_def.cell_size;
                    let expected_w = cs * sprite_def.frames;
                    let expected_h = cs * 4;
                    if w != expected_w || h != expected_h {
                        anyhow::bail!(
                            "PNG {id} ({category}): {w}×{h} (expected {expected_w}×{expected_h}, cell_size={cs}, frames={})",
                            sprite_def.frames
                        );
                    }
                }
                _ => anyhow::bail!(
                    "PNG {id} ({category}): unknown layout \"{}\"",
                    sprite_def.layout
                ),
            }
        }
    }

    Ok(())
}
