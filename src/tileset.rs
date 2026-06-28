//! Tileset, metatile, and palette data structures.

use bilge::prelude::*;
use binrw::{BinRead, BinReaderExt, BinResult, Endian};

use crate::error::{Error, Result};
use crate::graphics::{bgr555_to_rgba, Rgba};

/// Size of the GBA `MapLayout` struct in bytes.
pub const MAP_LAYOUT_SIZE: usize = 24;

/// Size of the GBA `Tileset` struct in bytes.
pub const TILESET_SIZE: usize = 24;

/// Size of a single 4bpp tile in bytes.
pub const TILE_SIZE: usize = 32;

/// Number of tiles in a metatile (2×2 = 4 bottom + 4 top).
pub const METATILE_LAYER_COUNT: usize = 8;

/// Number of colors in a single palette.
pub const PALETTE_COLORS: usize = 16;

/// Number of palettes in a tileset palette bank.
pub const PALETTE_COUNT: usize = 16;

/// Packed on-disk representation of a GBA `MapLayout`.
#[derive(Debug, Clone, BinRead)]
#[br(little)]
pub struct MapLayout {
    /// Map width in tiles.
    pub width: i32,
    /// Map height in tiles.
    pub height: i32,
    /// Pointer to border data.
    pub border_ptr: u32,
    /// Pointer to map data.
    pub map_ptr: u32,
    /// Pointer to the primary tileset.
    pub primary_tileset_ptr: u32,
    /// Pointer to the secondary tileset.
    pub secondary_tileset_ptr: u32,
}

/// Packed on-disk representation of a GBA `Tileset`.
#[derive(Debug, Clone, BinRead)]
#[br(little)]
pub struct TilesetHeader {
    /// Whether the tile graphics are LZ77 compressed.
    pub is_compressed: u8,
    /// Whether this is a secondary tileset.
    pub is_secondary: u8,
    /// 2 padding bytes.
    #[br(pad_size_to = 2)]
    pub _padding: (),
    /// Pointer to tile graphics.
    pub tiles_ptr: u32,
    /// Pointer to palette data.
    pub palettes_ptr: u32,
    /// Pointer to metatile data.
    pub metatiles_ptr: u32,
    /// Pointer to metatile attributes.
    pub metatile_attributes_ptr: u32,
    /// Pointer to the callback function.
    pub callback_ptr: u32,
}

/// One 8×8 tile of 4bpp graphics, stored as 32 raw bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileData {
    bytes: Vec<u8>,
}

impl TileData {
    /// Construct from raw 4bpp bytes. The buffer length is rounded down to
    /// a whole number of tiles; any trailing bytes are dropped.
    #[must_use]
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let len = (bytes.len() / TILE_SIZE) * TILE_SIZE;
        let mut bytes = bytes;
        bytes.truncate(len);
        Self { bytes }
    }

    /// Number of full 4bpp tiles in this buffer.
    #[must_use]
    pub fn tile_count(&self) -> usize {
        self.bytes.len() / TILE_SIZE
    }

    /// Returns `true` if the buffer contains no complete tiles.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Borrow the raw bytes for a single tile, or `None` if out of range.
    #[must_use]
    pub fn tile(&self, index: usize) -> Option<&[u8]> {
        let start = index * TILE_SIZE;
        let end = start + TILE_SIZE;
        self.bytes.get(start..end)
    }

    /// Returns the full backing buffer.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Iterate over each 32-byte tile.
    pub fn iter(&self) -> impl Iterator<Item = &[u8]> {
        self.bytes.chunks_exact(TILE_SIZE)
    }
}

/// One palette of 16 RGBA colors.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Palette(pub [Rgba; PALETTE_COLORS]);

impl Default for Palette {
    fn default() -> Self {
        Self([Rgba::TRANSPARENT; PALETTE_COLORS])
    }
}

impl Palette {
    /// Decode a palette from 32 bytes of BGR555 data (16 × `u16` LE).
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfRange`] if `bytes` is shorter than 32.
    pub fn from_bgr555(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < PALETTE_COLORS * 2 {
            return Err(Error::OutOfRange {
                offset: 0,
                size: bytes.len() as u32,
            });
        }
        let mut colors = [Rgba::TRANSPARENT; PALETTE_COLORS];
        for (i, color) in colors.iter_mut().enumerate() {
            let lo = u16::from(bytes[i * 2]);
            let hi = u16::from(bytes[i * 2 + 1]);
            *color = bgr555_to_rgba((hi << 8) | lo);
        }
        Ok(Self(colors))
    }

    /// Borrow the 16 colors.
    #[must_use]
    pub fn as_array(&self) -> &[Rgba; PALETTE_COLORS] {
        &self.0
    }
}

/// A bank of up to 16 palettes, decoded from a contiguous BGR555 block.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaletteData {
    palettes: Vec<Palette>,
}

impl PaletteData {
    /// Decode up to 16 palettes from a BGR555 block.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfRange`] if the block is shorter than 32 bytes.
    pub fn from_bgr555(bytes: &[u8]) -> Result<Self> {
        let count = (bytes.len() / (PALETTE_COLORS * 2)).min(PALETTE_COUNT);
        let mut palettes = Vec::with_capacity(count);
        for i in 0..count {
            let start = i * PALETTE_COLORS * 2;
            let end = start + PALETTE_COLORS * 2;
            palettes.push(Palette::from_bgr555(&bytes[start..end])?);
        }
        Ok(Self { palettes })
    }

    /// Number of palettes in this bank.
    #[must_use]
    pub fn len(&self) -> usize {
        self.palettes.len()
    }

    /// Returns `true` if the bank contains no palettes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.palettes.is_empty()
    }

    /// Look up a palette by index.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Palette> {
        self.palettes.get(index)
    }

    /// Iterate over all palettes.
    pub fn iter(&self) -> impl Iterator<Item = &Palette> {
        self.palettes.iter()
    }
}

/// Packed 16-bit metatile layer entry.
#[bitsize(16)]
#[derive(FromBits, Copy, Clone, PartialEq, Eq)]
pub struct MetatileLayerBits {
    /// Index into the tileset's 4bpp tile graphics.
    pub tile_index: u10,
    /// Horizontal flip flag.
    pub h_flip: bool,
    /// Vertical flip flag.
    pub v_flip: bool,
    /// Index into the tileset's palette bank.
    pub palette_index: u4,
}

/// Decoded metatile layer.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct MetatileLayer {
    /// Tile index into the tileset.
    pub tile_index: u16,
    /// Horizontal flip.
    pub h_flip: bool,
    /// Vertical flip.
    pub v_flip: bool,
    /// Palette index.
    pub palette_index: u8,
}

impl MetatileLayer {
    /// Decode a single packed layer from a `u16`.
    #[must_use]
    pub fn from_bits(raw: u16) -> Self {
        let bits = MetatileLayerBits::from(raw);
        Self {
            tile_index: bits.tile_index().value(),
            h_flip: bits.h_flip(),
            v_flip: bits.v_flip(),
            palette_index: bits.palette_index().value(),
        }
    }

    /// Re-encode the layer as a packed `u16`.
    #[must_use]
    pub fn to_bits(self) -> u16 {
        let bits = MetatileLayerBits::new(
            u10::new(self.tile_index),
            self.h_flip,
            self.v_flip,
            u4::new(self.palette_index),
        );
        u16::from(bits)
    }
}

/// A metatile: 8 layers (4 bottom + 4 top).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Metatile {
    /// 8 packed layers.
    pub layers: [MetatileLayer; METATILE_LAYER_COUNT],
}

impl Metatile {
    /// Decode a metatile from 16 raw bytes (8 × `u16` LE).
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut layers = [MetatileLayer {
            tile_index: 0,
            h_flip: false,
            v_flip: false,
            palette_index: 0,
        }; METATILE_LAYER_COUNT];
        for (i, layer) in layers.iter_mut().enumerate() {
            let lo = u16::from(bytes.get(i * 2).copied().unwrap_or(0));
            let hi = u16::from(bytes.get(i * 2 + 1).copied().unwrap_or(0));
            *layer = MetatileLayer::from_bits((hi << 8) | lo);
        }
        Self { layers }
    }
}

/// Decoded metatile bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetatileData {
    metatiles: Vec<Metatile>,
}

impl MetatileData {
    /// Decode metatiles from a packed byte block (16 bytes per metatile).
    #[must_use]
    pub fn from_packed(bytes: &[u8]) -> Self {
        let count = bytes.len() / (METATILE_LAYER_COUNT * 2);
        let mut metatiles = Vec::with_capacity(count);
        for i in 0..count {
            let start = i * METATILE_LAYER_COUNT * 2;
            let end = start + METATILE_LAYER_COUNT * 2;
            metatiles.push(Metatile::from_bytes(&bytes[start..end]));
        }
        Self { metatiles }
    }

    /// Number of metatiles in this bank.
    #[must_use]
    pub fn len(&self) -> usize {
        self.metatiles.len()
    }

    /// Returns `true` if the bank contains no metatiles.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.metatiles.is_empty()
    }

    /// Look up a metatile by index.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Metatile> {
        self.metatiles.get(index)
    }

    /// Iterate over all metatiles.
    pub fn iter(&self) -> impl Iterator<Item = &Metatile> {
        self.metatiles.iter()
    }
}

/// A fully decoded tileset, ready for rendering.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Tileset {
    /// Name of the tileset (e.g. `"Overworld"`).
    pub name: String,
    /// Whether this is a primary tileset (vs. secondary).
    pub is_primary: bool,
    /// Number of tiles in the tileset.
    pub tile_count: u16,
    /// Decompressed 4bpp tile graphics.
    #[serde(skip)]
    pub tiles: TileData,
    /// Decoded palette bank.
    #[serde(skip)]
    pub palettes: PaletteData,
    /// Decoded metatile bank.
    #[serde(skip)]
    pub metatiles: MetatileData,
}

/// Read a `BinRead`-derived struct from a ROM at a given address.
///
/// # Errors
///
/// Returns [`Error::OutOfRange`] if the address falls outside the ROM, or
/// [`Error::Io`] if the underlying read fails.
pub fn read_struct_at<T>(rom: &crate::Rom, address: u32) -> Result<T>
where
    for<'a> T: BinRead<Args<'a> = ()>,
{
    let offset = rom.offset_of(address)?;
    let mut cursor = std::io::Cursor::new(rom.bytes());
    use std::io::Seek;
    cursor.seek(std::io::SeekFrom::Start(offset as u64))?;
    let endian = Endian::Little;
    let value: T = cursor.read_type(endian)?;
    Ok(value)
}

/// Read a contiguous slice of `T` from a ROM starting at `address`.
///
/// # Errors
///
/// Returns [`Error::OutOfRange`] if the address is out of range or the slice
/// would overflow the ROM.
pub fn read_slice_at<T>(rom: &crate::Rom, address: u32, count: usize) -> Result<Vec<T>>
where
    T: BinRead<Args<'static> = ()> + 'static,
{
    let elem_size = std::mem::size_of::<T>();
    let _total = elem_size.checked_mul(count).ok_or(Error::OutOfRange {
        offset: address,
        size: 0,
    })?;
    let offset = rom.offset_of(address)?;
    let mut slice = Vec::with_capacity(count);
    let mut pos = offset;
    let bytes = rom.bytes();
    for _ in 0..count {
        let mut cursor = std::io::Cursor::new(&bytes[pos..pos + elem_size]);
        let value: T = cursor.read_type(Endian::Little)?;
        slice.push(value);
        pos += elem_size;
    }
    Ok(slice)
}

/// Read a `u32` pointer table (array of ROM addresses) starting at `address`.
///
/// # Errors
///
/// Returns [`Error::OutOfRange`] if the table extends past the end of the ROM.
pub fn read_ptr_table(rom: &crate::Rom, address: u32, count: usize) -> Result<Vec<u32>> {
    let total = count.checked_mul(4).ok_or(Error::OutOfRange {
        offset: address,
        size: 0,
    })?;
    let offset = rom.offset_of(address)?;
    if offset + total > rom.bytes().len() {
        return Err(Error::OutOfRange {
            offset: address,
            size: rom.bytes().len() as u32,
        });
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let pos = offset + i * 4;
        let b = &rom.bytes()[pos..pos + 4];
        let v = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        out.push(v);
    }
    Ok(out)
}

/// Read a single `u32` little-endian from a ROM address.
///
/// # Errors
///
/// Returns [`Error::OutOfRange`] if the address is out of range.
pub fn read_u32_at(rom: &crate::Rom, address: u32) -> Result<u32> {
    let offset = rom.offset_of(address)?;
    if offset + 4 > rom.bytes().len() {
        return Err(Error::OutOfRange {
            offset: address,
            size: rom.bytes().len() as u32,
        });
    }
    let b = &rom.bytes()[offset..offset + 4];
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

// Part of binrw's public surface that downstream code may re-export.
#[allow(dead_code)]
type _BinResultAlias = BinResult<()>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metatile_layer_roundtrip() {
        let layer = MetatileLayer {
            tile_index: 0x123,
            h_flip: true,
            v_flip: false,
            palette_index: 7,
        };
        let raw = layer.to_bits();
        let decoded = MetatileLayer::from_bits(raw);
        assert_eq!(decoded, layer);
    }

    #[test]
    fn metatile_layer_bit_layout() {
        // 0x4B12:  tile_index = 0x312, h=0, v=1, pal=4
        let bits = MetatileLayer::from_bits(0x4B12);
        assert_eq!(bits.tile_index, 0x312);
        assert!(!bits.h_flip);
        assert!(bits.v_flip);
        assert_eq!(bits.palette_index, 4);
    }

    #[test]
    fn metatile_from_bytes() {
        let bytes = [0u8; 16];
        let m = Metatile::from_bytes(&bytes);
        assert_eq!(m.layers.len(), 8);
    }

    #[test]
    fn palette_from_bgr555_pure_red() {
        // 0x001F = pure red
        let mut buf = vec![0u8; 32];
        buf[0] = 0x1F;
        buf[1] = 0x00;
        let palette = Palette::from_bgr555(&buf).unwrap();
        assert_eq!(palette.0[0], Rgba(255, 0, 0, 255));
    }

    #[test]
    fn palette_data_from_bgr555_16_palettes() {
        let bytes = vec![0u8; 16 * 16 * 2];
        let data = PaletteData::from_bgr555(&bytes).unwrap();
        assert_eq!(data.len(), 16);
    }

    #[test]
    fn tile_data_round_trip() {
        let bytes: Vec<u8> = (0..64).collect();
        let data = TileData::from_bytes(bytes);
        assert_eq!(data.tile_count(), 2);
        assert_eq!(data.tile(0).unwrap().len(), 32);
    }

    #[test]
    fn tile_data_drops_partial_tile() {
        let bytes: Vec<u8> = (0..40).collect();
        let data = TileData::from_bytes(bytes);
        assert_eq!(data.tile_count(), 1);
    }
}
