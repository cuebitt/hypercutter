//! Pokemon sprite data structures.

use binrw::BinRead;

use crate::graphics::Rgba;
use crate::tileset::PaletteData;
use crate::tileset::TileData;

/// Width and height of a Pokemon battle sprite frame in tiles.
pub const MON_PIC_WIDTH_TILES: u8 = 8;
/// Width and height of a Pokemon battle sprite frame in tiles.
pub const MON_PIC_HEIGHT_TILES: u8 = 8;

/// Pixels per side of a 4bpp Pokemon frame.
pub const MON_PIC_PIXELS: u16 = 64;
/// Bytes in a fully-populated 64×64 4bpp sprite frame.
pub const MON_PIC_BYTES: usize = 2048;
/// Bytes in a fully-populated 64×64 4bpp sprite frame, as a `u16`
/// (used by the form-packing heuristic).
pub const POKEMON_PIC_BYTES: usize = 2048;

/// Bytes in a single 16-color Pokemon palette.
pub const POKEMON_PALETTE_BYTES: usize = 32;

/// Identifier for a Pokemon species.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpeciesId(pub u16);

impl SpeciesId {
    /// Species index `0` is reserved for "no species" in the GBA games.
    pub const NONE: Self = Self(0);
    /// Traditional "egg" species slot.
    pub const EGG: Self = Self(412);
}

impl std::fmt::Display for SpeciesId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// On-disk `MonCoords` struct (4 bytes, 2 padding).
#[derive(Debug, Clone, BinRead)]
#[br(little)]
pub struct MonCoordsOnDisk {
    /// Packed `(width_tiles << 4) | height_tiles`.
    pub size: u8,
    /// Vertical offset from the bottom of the 64×64 frame.
    pub y_offset: u8,
    /// Two padding bytes.
    #[br(pad_size_to = 2)]
    pub _padding: (),
}

/// Decoded mon coords.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct MonCoords {
    /// Sprite width in tiles.
    pub width_tiles: u8,
    /// Sprite height in tiles.
    pub height_tiles: u8,
    /// Vertical offset from the bottom of the 64×64 frame.
    pub y_offset: u8,
}

impl MonCoords {
    /// Decode from the on-disk packed representation.
    #[must_use]
    pub fn from_disk(disk: &MonCoordsOnDisk) -> Self {
        Self {
            width_tiles: (disk.size >> 4) & 0x0F,
            height_tiles: disk.size & 0x0F,
            y_offset: disk.y_offset,
        }
    }

    /// Width in pixels.
    #[must_use]
    pub fn width_pixels(&self) -> u32 {
        u32::from(self.width_tiles) * 8
    }

    /// Height in pixels.
    #[must_use]
    pub fn height_pixels(&self) -> u32 {
        u32::from(self.height_tiles) * 8
    }
}

/// A single Pokemon battle sprite sheet (64×64 frame at 4bpp).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteSheet {
    /// Tile graphics (decompressed).
    pub tiles: TileData,
    /// Front-vs-back coords (size + y offset).
    pub coords: MonCoords,
}

/// A full base-species sprite (front, back, normal and shiny palettes).
#[derive(Debug, Clone)]
pub struct Sprite {
    /// Species id.
    pub id: SpeciesId,
    /// Lowercase species name.
    pub name: String,
    /// Front sprite sheet, if present.
    pub front: Option<SpriteSheet>,
    /// Back sprite sheet, if present.
    pub back: Option<SpriteSheet>,
    /// Normal palette.
    pub palette: PaletteData,
    /// Shiny palette.
    pub shiny_palette: PaletteData,
    /// Front sprite coords.
    pub front_coords: MonCoords,
    /// Back sprite coords.
    pub back_coords: MonCoords,
}

/// An alternate-form sprite (e.g. Unown B, Deoxys Attack).
#[derive(Debug, Clone)]
pub struct FormSprite {
    /// Base species id.
    pub base: SpeciesId,
    /// Form identifier (e.g. "B" for UnownB, "1" for Castform forms).
    pub form: String,
    /// Decompressed front tile data, if any.
    pub front_tiles: Option<Vec<u8>>,
    /// Decompressed back tile data, if any.
    pub back_tiles: Option<Vec<u8>>,
    /// Normal palette, if any.
    pub palette: Option<PaletteData>,
    /// Shiny palette, if any.
    pub shiny_palette: Option<PaletteData>,
}

/// Sprite data returned to JS via the WASM boundary.
#[derive(Debug, Clone)]
pub struct SpriteExport {
    /// `id`
    pub id: u16,
    /// `name`
    pub name: String,
    /// RGBA pixel data for the front sprite (64×64×4 bytes), if any.
    pub front_rgba: Option<Vec<u8>>,
    /// RGBA pixel data for the back sprite (64×64×4 bytes), if any.
    pub back_rgba: Option<Vec<u8>>,
    /// RGBA bytes for the normal palette (16 colors × 4 bytes), if any.
    pub palette_rgba: Option<Vec<Rgba>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn species_id_constants() {
        assert_eq!(SpeciesId::NONE.0, 0);
        assert_eq!(SpeciesId::EGG.0, 412);
    }

    #[test]
    fn mon_coords_decode() {
        // size = 0x45 → width=4, height=5
        let disk = MonCoordsOnDisk {
            size: 0x45,
            y_offset: 14,
            _padding: (),
        };
        let c = MonCoords::from_disk(&disk);
        assert_eq!(c.width_tiles, 4);
        assert_eq!(c.height_tiles, 5);
        assert_eq!(c.y_offset, 14);
        assert_eq!(c.width_pixels(), 32);
        assert_eq!(c.height_pixels(), 40);
    }
}
