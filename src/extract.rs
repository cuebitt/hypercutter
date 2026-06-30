//! High-level extraction facade.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use binrw::BinRead;


use crate::error::{Error, Result};
use crate::lzss::decompress as decompress_lzss;
use crate::lzss::is_lzss;
use crate::rom::Rom;
use crate::sprite::{
    Footprint, FormSprite, MonCoords, MonCoordsOnDisk, OverworldFrame, OverworldSprite,
    SpeciesId, Sprite, SpriteSheet, POKEMON_PALETTE_BYTES, POKEMON_PIC_BYTES,
};
use crate::symbols::SymbolTable;
use crate::tileset::{
    read_ptr_table, read_struct_at, MetatileData, PaletteData, TileData, Tileset, TilesetHeader,
};

/// Default number of tiles in a primary tileset (Emerald, Ruby, Sapphire).
const DEFAULT_PRIMARY_TILE_COUNT: u16 = 0x200;

/// Maximum bytes to read when probing for LZSS-compressed data.
const MAX_LZSS_READ: usize = 0x10000;

/// Maximum bytes to read when probing for a compressed palette block.
const MAX_PALETTE_READ: usize = 0x1000;

/// Species indices at or above this threshold are alternate-form slots
/// handled by the `forms()` method rather than `sprites()`.
const FORM_SPECIES_THRESHOLD: u16 = 413;

/// Length of a species name field in the `gSpeciesNames` table.
const SPECIES_NAME_LENGTH: usize = 11;

/// Sprite-table symbol names we need for extraction.
const SPRITE_SYMBOL_NAMES: &[&str] = &[
    "gMonFrontPicTable",
    "gMonBackPicTable",
    "gMonPaletteTable",
    "gMonShinyPaletteTable",
    "gMonFrontPicCoords",
    "gMonBackPicCoords",
    "gMonFootprintTable",
];

/// Overworld sprite symbol names we need for extraction.
const OVERWORLD_SYMBOL_NAMES: &[&str] = &["gObjectEventGraphicsInfoPointers"];

/// Size of the `ObjectEventGraphicsInfo` struct in bytes (0x24).
const OBJ_EVENT_GFX_INFO_SIZE: usize = 0x24;

/// On-disk `ObjectEventGraphicsInfo` struct (36 bytes).
///
/// Matches the C struct in `include/global.fieldmap.h` of the pret projects.
#[derive(Debug, Clone, BinRead)]
#[br(little)]
#[allow(dead_code)]
struct ObjectEventGraphicsInfoRaw {
    tile_tag: u16,
    palette_tag: u16,
    reflection_palette_tag: u16,
    size: u16,
    width: u16,
    height: u16,
    flags: u8,
    tracks: u8,
    _padding: u16,
    oam_ptr: u32,
    subsprite_tables_ptr: u32,
    anims_ptr: u32,
    images_ptr: u32,
    affine_anims_ptr: u32,
}

/// Configuration for extraction.
#[derive(Debug, Clone)]
pub struct ExtractOptions {
    /// Number of tiles in the primary tileset.
    pub primary_tile_count: u16,
    /// Tileset names to exclude from the output.
    pub exclude_tilesets: Vec<String>,
    /// Whether to include alternate-form sprites.
    pub include_forms: bool,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            primary_tile_count: DEFAULT_PRIMARY_TILE_COUNT,
            exclude_tilesets: Vec::new(),
            include_forms: false,
        }
    }
}

/// All metatiles extracted from a ROM, indexed by name.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Metatiles {
    /// `name -> (primary, secondary)`. Either may be missing for tilesets
    /// that no map uses.
    #[serde(flatten)]
    pub entries: BTreeMap<String, MetatileEntry>,
}

/// One metatile entry: a primary tileset plus an optional secondary.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MetatileEntry {
    /// Primary tileset (always present).
    pub primary: Tileset,
    /// Optional secondary tileset, when a map combines the two.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary: Option<Tileset>,
}

impl Metatiles {
    /// Look up an entry by metatile name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&MetatileEntry> {
        self.entries.get(name)
    }

    /// Iterate over `(name, entry)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &MetatileEntry)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Iterate over all entry names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if there are no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// High-level extraction facade.
///
/// Created from a [`Rom`] and a [`SymbolTable`]. Use the methods to extract
/// tilesets, metatiles, and Pokemon sprites.
#[derive(Debug)]
pub struct Extractor<'rom> {
    rom: &'rom Rom,
    symbols: &'rom SymbolTable,
    options: ExtractOptions,
}

impl<'rom> Extractor<'rom> {
    /// Create a new `Extractor` with default options.
    #[must_use]
    pub const fn new(rom: &'rom Rom, symbols: &'rom SymbolTable) -> Self {
        Self {
            rom,
            symbols,
            options: ExtractOptions {
                primary_tile_count: DEFAULT_PRIMARY_TILE_COUNT,
                exclude_tilesets: Vec::new(),
                include_forms: false,
            },
        }
    }

    /// Override the extraction options.
    #[must_use]
    pub fn with_options(mut self, options: ExtractOptions) -> Self {
        self.options = options;
        self
    }

    /// Validate that critical symbols point to readable ROM data.
    ///
    /// Checks that `gTileset_General` (or the first available tileset)
    /// contains a parseable `TilesetHeader` with in-range pointers. Returns
    /// `true` when the symbol table matches the loaded ROM, `false` when
    /// addresses are garbage (wrong ROM revision).
    #[must_use]
    pub fn validate(&self) -> bool {
        // Check a known tileset symbol to verify the .sym file matches this ROM.
        if let Some(sym) = self.symbols.get("gTileset_General") {
            return self.validate_tileset_sym(sym);
        }
        // Fallback: try the first gTileset_ symbol in the table.
        for sym in self.symbols.iter() {
            if sym.name.starts_with("gTileset_") {
                return self.validate_tileset_sym(sym);
            }
        }
        false
    }

    fn validate_tileset_sym(&self, sym: &crate::Symbol) -> bool {
        let header: crate::tileset::TilesetHeader = match read_struct_at(self.rom, sym.address) {
            Ok(h) => h,
            Err(_) => return false,
        };
        let tiles_ok = self.rom.offset_of(header.tiles_ptr).is_ok();
        let pal_ok = self.rom.offset_of(header.palettes_ptr).is_ok();
        let met_ok = self.rom.offset_of(header.metatiles_ptr).is_ok();
        tiles_ok && pal_ok && met_ok
    }

    /// Returns the configured primary-tile count.
    #[must_use]
    pub const fn primary_tile_count(&self) -> u16 {
        self.options.primary_tile_count
    }

    /// Returns the configured exclusion list.
    #[must_use]
    pub fn exclude_tilesets(&self) -> &[String] {
        &self.options.exclude_tilesets
    }

    /// Returns the underlying ROM.
    #[must_use]
    pub const fn rom(&self) -> &'rom Rom {
        self.rom
    }

    /// Returns the underlying symbol table.
    #[must_use]
    pub const fn symbols(&self) -> &'rom SymbolTable {
        self.symbols
    }

    /// Extract all metatiles for the loaded ROM.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SymbolNotFound`] if the `.sym` file is missing
    /// `Start` or `gMapLayouts`. Returns [`Error::OutOfRange`] if a pointer
    /// in the ROM falls outside the loaded bytes. Returns
    /// [`Error::Decompression`] if LZSS decompression of a compressed
    /// tileset fails.
    pub fn metatiles(&self) -> Result<Metatiles> {
        let tilesets = self.tilesets()?;
        let pairs = self.tileset_name_pairs()?;
        let mut metatiles = Metatiles::default();

        for sym in self.symbols.iter() {
            let Some(name) = sym.name.strip_prefix("gMetatiles_") else {
                continue;
            };
            if self.options.exclude_tilesets.iter().any(|n| n == name) {
                continue;
            }
            let primary_name = find_primary(&pairs, name).unwrap_or(name);
            let Some(primary) = tilesets.get(primary_name) else {
                continue;
            };
            let entry = if primary_name != name {
                if let Some(secondary) = tilesets.get(name) {
                    MetatileEntry {
                        primary: primary.clone(),
                        secondary: Some(secondary.clone()),
                    }
                } else {
                    continue;
                }
            } else {
                MetatileEntry {
                    primary: primary.clone(),
                    secondary: None,
                }
            };
            metatiles.entries.insert(name.to_owned(), entry);
        }
        Ok(metatiles)
    }

    /// Extract every tileset referenced by `gTileset_*` symbols.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfRange`] if a tileset pointer falls outside the
    /// ROM. Returns [`Error::Decompression`] if LZSS decompression fails
    /// for a compressed tileset.
    pub fn tilesets(&self) -> Result<BTreeMap<String, Tileset>> {
        let mut out = BTreeMap::new();
        for sym in self.symbols.iter() {
            let Some(name) = sym.name.strip_prefix("gTileset_") else {
                continue;
            };
            if self.options.exclude_tilesets.iter().any(|n| n == name) {
                continue;
            }
            let Ok(tileset) = self.load_tileset(name, sym.address) else {
                continue;
            };
            out.insert(name.to_owned(), tileset);
        }
        Ok(out)
    }

    fn load_tileset(&self, name: &str, address: u32) -> Result<Tileset> {
        let header = read_struct_at::<TilesetHeader>(self.rom, address)?;
        let tileset_info = self.tileset_lengths(name);
        let metatile_length = self.metatile_length(name);

        let raw_tiles = self.read_field(
            header.tiles_ptr,
            tileset_info.tiles_length,
            header.is_compressed != 0,
        )?;
        let tiles = TileData::from_bytes(raw_tiles);
        let tile_count = tiles.tile_count() as u16;

        let raw_palettes = self.read_field(header.palettes_ptr, 16 * 16 * 2, false)?;
        let palettes = PaletteData::from_bgr555(&raw_palettes)?;

        let raw_metatiles = self.read_field(header.metatiles_ptr, metatile_length, false)?;
        let metatiles = MetatileData::from_packed(&raw_metatiles);

        Ok(Tileset {
            name: name.to_owned(),
            is_primary: header.is_secondary == 0,
            tile_count,
            tiles,
            palettes,
            metatiles,
        })
    }

    /// Read a single field (tiles, palettes, metatiles) from the ROM,
    /// honoring the LZSS magic byte when compressed.
    ///
    /// When `is_compressed` is true the data is decompressed via LZSS and the
    /// full result is returned **without** truncating to `length`.  Callers
    /// that need a specific size (e.g. sprites) should truncate themselves.
    fn read_field(&self, ptr: u32, length: usize, is_compressed: bool) -> Result<Vec<u8>> {
        let offset = match self.rom.offset_of(ptr) {
            Ok(o) => o,
            Err(_) => return Ok(Vec::new()),
        };
        let bytes = self.rom.bytes();
        if offset >= bytes.len() {
            return Ok(Vec::new());
        }
        if is_compressed {
            if offset + 1 > bytes.len() {
                return Ok(Vec::new());
            }
            if !is_lzss(&bytes[offset..]) {
                let end = (offset + length).min(bytes.len());
                return Ok(bytes[offset..end].to_vec());
            }
            let max_read = MAX_LZSS_READ.min(bytes.len() - offset);
            let compressed = &bytes[offset..offset + max_read];
            let decompressed = decompress_lzss(compressed)?;
            Ok(decompressed)
        } else {
            let end = (offset + length).min(bytes.len());
            Ok(bytes[offset..end].to_vec())
        }
    }

    /// Read tileset-related lengths from the symbol table.
    #[must_use]
    pub fn tileset_lengths(&self, name: &str) -> TilesetInfo {
        let mut info = TilesetInfo::default();
        let variants: &[&str] = if name == "Building" {
            &["Building", "InsideBuilding"]
        } else if name == "InsideBuilding" {
            &["InsideBuilding", "Building"]
        } else {
            &[name]
        };
        for variant in variants {
            if let Some(sym) = self.symbols.get(&format!("gTilesetTiles_{variant}")) {
                info.tiles_length = sym.length as usize;
            }
            if let Some(sym) = self.symbols.get(&format!("gTilesetPalettes_{variant}")) {
                info.palettes_length = sym.length as usize;
            }
            if info.tiles_length != 0 || info.palettes_length != 0 {
                break;
            }
        }
        info
    }

    /// Read metatile length from the symbol table.
    #[must_use]
    pub fn metatile_length(&self, name: &str) -> usize {
        self.symbols
            .get(&format!("gMetatiles_{name}"))
            .map_or(0, |s| s.length as usize)
    }

    /// Build a mapping `primary_name -> [secondary_name, ...]` from the
    /// map table.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SymbolNotFound`] if `Start` or `gMapLayouts` is
    /// missing, or [`Error::OutOfRange`] if a map layout pointer falls
    /// outside the ROM.
    pub fn tileset_name_pairs(&self) -> Result<BTreeMap<String, Vec<String>>> {
        let start_sym = self
            .symbols
            .get("Start")
            .ok_or(Error::SymbolNotFound { name: "Start" })?;
        let map_table_sym = self
            .symbols
            .get("gMapLayouts")
            .ok_or(Error::SymbolNotFound {
                name: "gMapLayouts",
            })?;
        let map_count = (map_table_sym.length as usize) / 4;

        let start = start_sym.address;
        let map_table_offset = map_table_sym.address - start;
        let map_pointers = read_ptr_table(self.rom, start + map_table_offset, map_count)?;

        let mut primary_to_secondaries: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
        for &ptr in &map_pointers {
            if ptr <= start {
                continue;
            }
            let layout = read_struct_at::<crate::tileset::MapLayout>(self.rom, ptr)?;
            primary_to_secondaries
                .entry(layout.primary_tileset_ptr)
                .or_default()
                .insert(layout.secondary_tileset_ptr);
        }

        let mut pairs: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (primary_addr, secondaries) in primary_to_secondaries {
            let Some(primary_sym) = self.symbols.by_address(primary_addr) else {
                continue;
            };
            let primary_name = primary_sym
                .name
                .strip_prefix("gTileset_")
                .unwrap_or(&primary_sym.name)
                .to_owned();
            let mut names = Vec::new();
            for sec_addr in secondaries {
                if let Some(sec_sym) = self.symbols.by_address(sec_addr) {
                    let n = sec_sym
                        .name
                        .strip_prefix("gTileset_")
                        .unwrap_or(&sec_sym.name)
                        .to_owned();
                    names.push(n);
                }
            }
            pairs.insert(primary_name, names);
        }
        Ok(pairs)
    }

    /// Extract all base-species sprites.
    ///
    /// Skips placeholder species (index 0), unknown entries past the name
    /// table, and the alternate-form slots (413+) that are handled
    /// separately by [`Self::forms`].  Species 412 (Egg) is kept.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SymbolNotFound`] for any missing sprite-table symbol
    /// (`gMonFrontPicTable`, `gMonBackPicTable`, `gMonPaletteTable`,
    /// `gMonShinyPaletteTable`, `gMonFrontPicCoords`, `gMonBackPicCoords`).
    /// Returns [`Error::OutOfRange`] or [`Error::Decompression`] if a
    /// compressed sprite data block cannot be read.
    pub fn sprites(&self) -> Result<Vec<Sprite>> {
        let start_sym = self
            .symbols
            .get("Start")
            .ok_or(Error::SymbolNotFound { name: "Start" })?;
        let start = start_sym.address;

        let mut addrs = BTreeMap::new();
        for &name in SPRITE_SYMBOL_NAMES {
            let sym = self
                .symbols
                .get(name)
                .ok_or(Error::SymbolNotFound { name })?;
            addrs.insert(name, (sym.address - start) as usize);
        }

        let front_len = self
            .symbols
            .get("gMonFrontPicTable")
            .ok_or(Error::SymbolNotFound {
                name: "gMonFrontPicTable",
            })?
            .length as usize;
        let count = front_len / 8;
        let species_names = self.species_names()?;

        let mut out = Vec::with_capacity(count);
        for species_id in 0..count {
            // Species 413+ are alternate-form slots handled by forms().
            if species_id >= FORM_SPECIES_THRESHOLD as usize {
                continue;
            }
            let id = SpeciesId(species_id as u16);
            if let Some(sprite) = self.read_base_sprite(id, &addrs, &species_names)? {
                out.push(sprite);
            }
        }
        Ok(out)
    }

    fn read_base_sprite(
        &self,
        id: SpeciesId,
        addrs: &BTreeMap<&'static str, usize>,
        species_names: &[String],
    ) -> Result<Option<Sprite>> {
        let front_offset = addrs["gMonFrontPicTable"];
        let back_offset = addrs["gMonBackPicTable"];
        let palette_offset = addrs["gMonPaletteTable"];
        let shiny_offset = addrs["gMonShinyPaletteTable"];
        let front_coords_offset = addrs["gMonFrontPicCoords"];
        let back_coords_offset = addrs["gMonBackPicCoords"];

        let front = self.read_compressed_sprite_sheet(front_offset, id.0 as usize)?;
        let back = self.read_compressed_sprite_sheet(back_offset, id.0 as usize)?;
        if front.is_none() && back.is_none() {
            return Ok(None);
        }

        let name = species_names
            .get(id.0 as usize)
            .cloned()
            .unwrap_or_default();

        let front = match front {
            Some(f) => f,
            None => return Ok(None),
        };
        let back = match back {
            Some(b) => b,
            None => return Ok(None),
        };

        // Species with no name entry in the ROM table but with valid sprite
        // data: use a contextual fallback.
        let name = if name.is_empty() {
            match id.0 {
                0 => "question".to_owned(),
                412 => "egg".to_owned(),
                _ => return Ok(None),
            }
        } else {
            name
        };

        let palette = self
            .read_compressed_sprite_palette(palette_offset, id.0 as usize)?
            .unwrap_or_default();
        let shiny_palette = self
            .read_compressed_sprite_palette(shiny_offset, id.0 as usize)?
            .unwrap_or_default();

        let front_coords = self
            .read_mon_coords(front_coords_offset, id.0 as usize)?
            .unwrap_or_default();
        let back_coords = self
            .read_mon_coords(back_coords_offset, id.0 as usize)?
            .unwrap_or_default();

        let footprint = self.read_footprint(id, addrs);

        let front_sheet = SpriteSheet {
            tiles: TileData::from_bytes(front),
            coords: front_coords,
        };
        let back_sheet = SpriteSheet {
            tiles: TileData::from_bytes(back),
            coords: back_coords,
        };

        Ok(Some(Sprite {
            id,
            name,
            front: Some(front_sheet),
            back: Some(back_sheet),
            palette,
            shiny_palette,
            front_coords,
            back_coords,
            footprint,
        }))
    }

    fn read_compressed_sprite_sheet(
        &self,
        table_offset: usize,
        species_index: usize,
    ) -> Result<Option<Vec<u8>>> {
        let elem_offset = table_offset + species_index * 8;
        if elem_offset + 8 > self.rom.bytes().len() {
            return Ok(None);
        }
        let bytes = &self.rom.bytes()[elem_offset..elem_offset + 8];
        let data_ptr = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let size = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
        if data_ptr == 0 {
            return Ok(None);
        }
        let mut raw = self.read_field(data_ptr, size, true)?;
        raw.truncate(size);
        Ok(Some(raw))
    }

    fn read_compressed_sprite_palette(
        &self,
        table_offset: usize,
        species_index: usize,
    ) -> Result<Option<PaletteData>> {
        let elem_offset = table_offset + species_index * 8;
        if elem_offset + 8 > self.rom.bytes().len() {
            return Ok(None);
        }
        let bytes = &self.rom.bytes()[elem_offset..elem_offset + 8];
        let data_ptr = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if data_ptr == 0 {
            return Ok(None);
        }
        let raw = self.read_field(data_ptr, POKEMON_PALETTE_BYTES, true)?;
        Ok(Some(PaletteData::from_bgr555(&raw)?))
    }

    fn read_mon_coords(
        &self,
        table_offset: usize,
        species_index: usize,
    ) -> Result<Option<MonCoords>> {
        let elem_offset = table_offset + species_index * 4;
        if elem_offset + 4 > self.rom.bytes().len() {
            return Ok(None);
        }
        let mut cursor = std::io::Cursor::new(&self.rom.bytes()[elem_offset..elem_offset + 4]);
        let disk = MonCoordsOnDisk::read_le(&mut cursor)?;
        Ok(Some(MonCoords::from_disk(&disk)))
    }

    /// Read a footprint image for a species.
    ///
    /// Footprints are 16×16 1bpp images (32 bytes uncompressed) stored in
    /// the `gMonFootprintTable` (array of `u8*` pointers).
    fn read_footprint(&self, id: SpeciesId, addrs: &BTreeMap<&'static str, usize>) -> Option<Footprint> {
        let table_offset = *addrs.get("gMonFootprintTable")?;
        let elem_offset = table_offset + id.0 as usize * 4;
        if elem_offset + 4 > self.rom.bytes().len() {
            return None;
        }
        let b = &self.rom.bytes()[elem_offset..elem_offset + 4];
        let data_ptr = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        if data_ptr == 0 {
            return None;
        }
        let data_offset = match self.rom.offset_of(data_ptr) {
            Ok(o) => o,
            Err(_) => return None,
        };
        if data_offset + 32 > self.rom.bytes().len() {
            return None;
        }
        let mut data = [0u8; 32];
        data.copy_from_slice(&self.rom.bytes()[data_offset..data_offset + 32]);
        Some(Footprint { data })
    }

    /// Read all species names from the `gSpeciesNames` table.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SymbolNotFound`] if `gSpeciesNames` or `Start` is
    /// missing.
    pub fn species_names(&self) -> Result<Vec<String>> {
        let start_sym = self
            .symbols
            .get("Start")
            .ok_or(Error::SymbolNotFound { name: "Start" })?;
        let start = start_sym.address;
        let sym = self
            .symbols
            .get("gSpeciesNames")
            .ok_or(Error::SymbolNotFound {
                name: "gSpeciesNames",
            })?;
        let offset = (sym.address - start) as usize;
        let name_length = SPECIES_NAME_LENGTH;
        let count = (sym.length as usize) / name_length;
        let mut names = Vec::with_capacity(count);
        for i in 0..count {
            let pos = offset + i * name_length;
            if pos + name_length > self.rom.bytes().len() {
                break;
            }
            let raw = &self.rom.bytes()[pos..pos + name_length];
            names.push(decode_species_name(raw));
        }
        Ok(names)
    }

    /// Read the national Pokédex number mapping.
    ///
    /// Returns a `Vec` where index = internal species ID and value = National
    /// Dex number. The table in the ROM is 1-indexed (species 0 has no
    /// entry), so the returned vec has `count + 1` entries with index 0
    /// set to 0.
    ///
    /// Tries all known symbol names across games and picks the longest
    /// match to avoid a small lookup table shadowing the real mapping:
    /// - `sSpeciesToNationalPokedexNum` (FireRed/LeafGreen local)
    /// - `gSpeciesToNationalPokedexNum` (Ruby/Sapphire global)
    /// - `SpeciesToNationalPokedexNum` (all games, often a short lookup)
    ///
    /// # Errors
    ///
    /// Returns [`Error::SymbolNotFound`] if none of the symbols is present.
    pub fn national_dex_map(&self) -> Result<Vec<u16>> {
        let start_sym = self
            .symbols
            .get("Start")
            .ok_or(Error::SymbolNotFound { name: "Start" })?;
        let start = start_sym.address;
        let candidates = [
            "sSpeciesToNationalPokedexNum",
            "gSpeciesToNationalPokedexNum",
            "SpeciesToNationalPokedexNum",
        ];
        let sym = candidates
            .iter()
            .filter_map(|name| self.symbols.get(name))
            .max_by_key(|s| s.length)
            .ok_or(Error::SymbolNotFound {
                name: "sSpeciesToNationalPokedexNum",
            })?;
        let offset = (sym.address - start) as usize;
        let count = (sym.length as usize) / 2;
        let mut map = vec![0u16; count + 1];
        for i in 0..count {
            let pos = offset + i * 2;
            if pos + 2 > self.rom.bytes().len() {
                break;
            }
            map[i + 1] = u16::from_le_bytes([self.rom.bytes()[pos], self.rom.bytes()[pos + 1]]);
        }
        Ok(map)
    }

    /// Extract alternate-form sprites using the packed-form heuristic.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SymbolNotFound`] if `Start` is missing. Returns
    /// [`Error::Decompression`] if a form sprite data block cannot be
    /// decompressed.
    pub fn forms(&self) -> Result<Vec<FormSprite>> {
        let start_sym = self
            .symbols
            .get("Start")
            .ok_or(Error::SymbolNotFound { name: "Start" })?;
        let start = start_sym.address;
        let species_names = self.species_names()?;

        let mut by_key: BTreeMap<(String, String), FormSprite> = BTreeMap::new();

        // Pass 1: individual form symbols.  Base-species symbols (whose
        // suffix matches a species name exactly) are skipped; form symbols
        // (UnownB, CastformRain, etc.) are kept even if their address
        // appears in the main tables (species 413+ in Emerald).
        for sym in self.symbols.iter() {
            let prefix = if sym.name.starts_with("gMonFrontPic_") {
                "gMonFrontPic_"
            } else if sym.name.starts_with("gMonBackPic_") {
                "gMonBackPic_"
            } else {
                continue;
            };
            let suffix = &sym.name[prefix.len()..];
            // Skip base species (suffix matches a name exactly).
            let lower = suffix.to_ascii_lowercase();
            if species_names.iter().any(|n| n == &lower) {
                continue;
            }
            let Some((species, form)) = derive_species_form(suffix, &species_names) else {
                continue;
            };
            let key = (species.clone(), form.clone());
            let entry = by_key.entry(key).or_insert_with(|| FormSprite {
                base: SpeciesId(0),
                form,
                front_tiles: None,
                back_tiles: None,
                palette: None,
                shiny_palette: None,
            });
            entry.base = SpeciesId(species_index_of(&species, &species_names));
            let offset = (sym.address - start) as usize;
            let max_read = MAX_LZSS_READ.min(self.rom.bytes().len() - offset);
            let compressed = &self.rom.bytes()[offset..offset + max_read];
            let Ok(mut data) = decompress_lzss(compressed) else {
                continue;
            };
            if data.len() > POKEMON_PIC_BYTES {
                data.truncate(POKEMON_PIC_BYTES);
            }
            if prefix == "gMonFrontPic_" {
                entry.front_tiles = Some(data);
            } else {
                entry.back_tiles = Some(data);
            }
        }

        // Pass 2: packed species with multiple forms in the front+back data.
        for sym in self.symbols.iter() {
            let Some(suffix) = sym.name.strip_prefix("gMonFrontPic_") else {
                continue;
            };
            let lower = suffix.to_ascii_lowercase();
            if !species_names.iter().any(|n| n == &lower) {
                continue;
            }
            let back_sym_name = format!("gMonBackPic_{suffix}");
            let Some(back_sym) = self.symbols.get(&back_sym_name) else {
                continue;
            };
            let front_offset = (sym.address - start) as usize;
            let back_offset = (back_sym.address - start) as usize;
            let Ok(front_data) = decompress_lzss(slice_lzss(self.rom, front_offset)) else {
                continue;
            };
            let Ok(back_data) = decompress_lzss(slice_lzss(self.rom, back_offset)) else {
                continue;
            };
            let front_forms = front_data.len() / POKEMON_PIC_BYTES;
            let back_forms = back_data.len() / POKEMON_PIC_BYTES;
            if front_forms < 2 || back_forms < 2 {
                continue;
            }
            let form_count = front_forms.min(back_forms);
            for i in 0..form_count {
                let key = (lower.clone(), i.to_string());
                let entry = by_key.entry(key).or_insert_with(|| FormSprite {
                    base: SpeciesId(species_index_of(&lower, &species_names)),
                    form: i.to_string(),
                    front_tiles: None,
                    back_tiles: None,
                    palette: None,
                    shiny_palette: None,
                });
                let s = i * POKEMON_PIC_BYTES;
                let e = s + POKEMON_PIC_BYTES;
                if e <= front_data.len() {
                    entry.front_tiles = Some(front_data[s..e].to_vec());
                }
                if e <= back_data.len() {
                    entry.back_tiles = Some(back_data[s..e].to_vec());
                }
            }
        }

        // Pass 3: per-form palettes.
        let palette_species: BTreeSet<String> =
            by_key.keys().map(|(species, _)| species.clone()).collect();
        for sym in self.symbols.iter() {
            let Some(suffix) = sym.name.strip_prefix("gMonPalette_") else {
                continue;
            };
            let lower = suffix.to_ascii_lowercase();
            if !palette_species.contains(&lower) {
                continue;
            }
            let offset = (sym.address - start) as usize;
            let max_read = MAX_PALETTE_READ.min(self.rom.bytes().len() - offset);
            let compressed = &self.rom.bytes()[offset..offset + max_read];
            let Ok(pal_data) = decompress_lzss(compressed) else {
                continue;
            };
            let count = pal_data.len() / POKEMON_PALETTE_BYTES;
            for ((species, form), entry) in by_key.iter_mut() {
                if species != &lower {
                    continue;
                }
                if count > 1 {
                    if let Ok(idx) = form.parse::<usize>() {
                        let s = idx * POKEMON_PALETTE_BYTES;
                        let e = s + POKEMON_PALETTE_BYTES;
                        if e <= pal_data.len() {
                            if let Ok(p) = PaletteData::from_bgr555(&pal_data[s..e]) {
                                entry.palette = Some(p);
                            }
                            continue;
                        }
                    }
                }
                if let Ok(p) = PaletteData::from_bgr555(&pal_data) {
                    entry.palette = Some(p);
                }
            }
        }

        // Pass 4: shiny palettes.
        for sym in self.symbols.iter() {
            let Some(suffix) = sym.name.strip_prefix("gMonShinyPalette_") else {
                continue;
            };
            let lower = suffix.to_ascii_lowercase();
            if !palette_species.contains(&lower) {
                continue;
            }
            let offset = (sym.address - start) as usize;
            let max_read = MAX_PALETTE_READ.min(self.rom.bytes().len() - offset);
            let compressed = &self.rom.bytes()[offset..offset + max_read];
            let Ok(pal_data) = decompress_lzss(compressed) else {
                continue;
            };
            for ((species, _), entry) in by_key.iter_mut() {
                if species == &lower {
                    if let Ok(p) = PaletteData::from_bgr555(&pal_data) {
                        entry.shiny_palette = Some(p);
                    }
                }
            }
        }

        Ok(by_key.into_values().collect())
    }

    /// Extract all overworld object event sprites.
    ///
    /// Reads the `gObjectEventGraphicsInfoPointers` table to discover every
    /// overworld sprite, decompresses tile data and palettes, and splits
    /// multi-frame sprites into individual [`OverworldFrame`] entries.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SymbolNotFound`] if `Start` or the pointer table
    /// symbol is missing. Returns [`Error::OutOfRange`] or
    /// [`Error::Decompression`] if data cannot be read.
    pub fn overworld_sprites(&self) -> Result<Vec<OverworldSprite>> {
        let start_sym = self
            .symbols
            .get("Start")
            .ok_or(Error::SymbolNotFound { name: "Start" })?;
        let start = start_sym.address;

        let table_sym = self
            .symbols
            .get(OVERWORLD_SYMBOL_NAMES[0])
            .ok_or(Error::SymbolNotFound {
                name: OVERWORLD_SYMBOL_NAMES[0],
            })?;
        let table_offset = (table_sym.address - start) as usize;
        let num_entries = table_sym.length as usize / 4;

        let mut info_syms: Vec<&crate::symbols::Symbol> = self
            .symbols
            .iter()
            .filter(|s| s.name.starts_with("gObjectEventGraphicsInfo_"))
            .collect();
        info_syms.sort_by_key(|s| s.address);

        let pal_syms: HashMap<String, &crate::symbols::Symbol> = self
            .symbols
            .iter()
            .filter(|s| s.name.starts_with("gObjectEventPal_"))
            .filter(|s| !s.name.contains("Null"))
            .map(|s| {
                let key = s
                    .name
                    .strip_prefix("gObjectEventPal_")
                    .expect("gObjectEventPal_ prefix verified by filter")
                    .to_owned();
                (key, s)
            })
            .collect();

        let palette_map = self.build_palette_tag_map(start)?;

        let species_names = self.species_names().unwrap_or_default();

        let mut sprites = Vec::new();

        for idx in 0..num_entries {
            let ptr_pos = table_offset + idx * 4;
            if ptr_pos + 4 > self.rom.bytes().len() {
                break;
            }
            let b = &self.rom.bytes()[ptr_pos..ptr_pos + 4];
            let ptr = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            if ptr == 0 {
                continue;
            }
            let info_offset = match self.rom.offset_of(ptr) {
                Ok(o) => o,
                Err(_) => continue,
            };
            if info_offset + OBJ_EVENT_GFX_INFO_SIZE > self.rom.bytes().len() {
                continue;
            }
            let raw = &self.rom.bytes()[info_offset..info_offset + OBJ_EVENT_GFX_INFO_SIZE];
            // Manually parse the struct fields (little-endian) to avoid
            // potential binrw alignment issues with u32 fields.
            let info_size = u16::from_le_bytes([raw[6], raw[7]]);
            let info_width = u16::from_le_bytes([raw[8], raw[9]]);
            let info_height = u16::from_le_bytes([raw[10], raw[11]]);
            let info_images_ptr = u32::from_le_bytes([raw[28], raw[29], raw[30], raw[31]]);
            let info_palette_tag = u16::from_le_bytes([raw[2], raw[3]]);

            if info_size == 0 || info_width == 0 || info_height == 0 {
                continue;
            }

            // width and height are in pixels.
            // Convert to tiles for the struct fields.
            let width_tiles = info_width / 8;
            let height_tiles = info_height / 8;
            if width_tiles == 0 || height_tiles == 0 {
                continue;
            }
            // 4bpp: 32 bytes per 8x8 tile.
            let frame_size = width_tiles as usize * height_tiles as usize * 32;
            if frame_size == 0 {
                continue;
            }

            let name = info_syms
                .iter()
                .find(|s| {
                    let offset = (s.address - start) as usize;
                    offset == info_offset
                })
                .map(|s| {
                    s.name
                        .strip_prefix("gObjectEventGraphicsInfo_")
                        .unwrap_or(&s.name)
                        .to_owned()
                })
                .unwrap_or_else(|| format!("Unknown_{idx}"));

            let tile_data = self.read_frame_data(info_images_ptr);
            let tile_data = match tile_data {
                Some(d) => d,
                None => continue,
            };
            if tile_data.is_empty() {
                continue;
            }

            let frame_count = tile_data.len() / frame_size;
            if frame_count == 0 {
                continue;
            }

            let palette = self.resolve_palette(
                info_palette_tag,
                &name,
                &palette_map,
                &pal_syms,
                start,
            );
            let shiny_palette = self.resolve_shiny_palette(&name, &species_names);

            let mut frames = Vec::with_capacity(frame_count);
            for f in 0..frame_count {
                let offset = f * frame_size;
                let end = (offset + frame_size).min(tile_data.len());
                let raw = &tile_data[offset..end];
                let rearranged = rearrange_quadrant_tiles(raw, width_tiles, height_tiles);
                frames.push(OverworldFrame {
                    tiles: TileData::from_bytes(rearranged),
                    index: f as u16,
                });
            }

            if frames.is_empty() {
                continue;
            }

            sprites.push(OverworldSprite {
                id: idx as u16,
                name,
                width_tiles,
                height_tiles,
                total_size: info_size,
                frames,
                palette,
                shiny_palette,
            });
        }

        sprites.sort_by_key(|a| a.id);
        Ok(sprites)
    }



    /// Build a mapping from palette tag → palette data ROM address.
    ///
    /// Reads the `sObjectEventSpritePalettes` local symbol if available.
    fn build_palette_tag_map(
        &self,
        start: u32,
    ) -> Result<HashMap<u16, u32>> {
        let mut map = HashMap::new();
        let Some(sym) = self.symbols.get("sObjectEventSpritePalettes") else {
            return Ok(map);
        };
        let offset = (sym.address - start) as usize;
        let count = sym.length as usize / 8;
        for i in 0..count {
            let pos = offset + i * 8;
            if pos + 8 > self.rom.bytes().len() {
                break;
            }
            let b = &self.rom.bytes()[pos..pos + 8];
            let data_ptr = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            // The pret struct is { data, size, tag } but the table is
            // initialized as { ptr, tag_value } — the tag goes into the
            // `size` field; the `tag` field is zero-padded.
            let tag = u16::from_le_bytes([b[4], b[5]]);
            if tag != 0 && data_ptr != 0 {
                map.insert(tag, data_ptr);
            }
        }
        Ok(map)
    }

    /// Read tile data by following the `images` pointer in the graphics info.
    ///
    /// The `images_ptr` points to a `SpriteFrameImage` table. Each entry is
    /// `{ data_ptr: u32, size: u16, _pad: u16 }` (8 bytes, padded to align).
    /// We read the first entry's data pointer, find the matching pic symbol
    /// in the .sym file to get the data bounds, and decompress that range.
    fn read_frame_data(&self, images_ptr: u32) -> Option<Vec<u8>> {
        let images_offset = self.rom.offset_of(images_ptr).ok()?;
        let bytes = self.rom.bytes();

        // Read the first SpriteFrameImage entry to get the data pointer.
        if images_offset + 8 > bytes.len() {
            return None;
        }
        let b = &bytes[images_offset..images_offset + 4];
        let data_ptr = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        if data_ptr == 0 {
            return None;
        }

        // Find the symbol whose address matches this data pointer to get bounds.
        if let Some(sym) = self.symbols.by_address(data_ptr) {
            let data = self.read_field(data_ptr, sym.length as usize, true).ok()?;
            if data.is_empty() {
                None
            } else {
                Some(data)
            }
        } else {
            // Fallback: read up to 64KB
            let data = self.read_field(data_ptr, MAX_LZSS_READ, true).ok()?;
            if data.is_empty() {
                None
            } else {
                Some(data)
            }
        }
    }

    /// Resolve a palette for an overworld sprite given its palette tag.
    fn resolve_palette(
        &self,
        palette_tag: u16,
        name: &str,
        palette_map: &HashMap<u16, u32>,
        pal_syms: &HashMap<String, &crate::symbols::Symbol>,
        start: u32,
    ) -> PaletteData {
        if palette_tag == 0 {
            return PaletteData::default();
        }
        if let Some(&data_ptr) = palette_map.get(&palette_tag) {
            if let Ok(raw) = self.read_field(data_ptr, 32, true) {
                if let Ok(p) = PaletteData::from_bgr555(&raw) {
                    return p;
                }
            }
        }
        if let Some(sym) = pal_syms.get(name) {
            let offset = (sym.address - start) as usize;
            if offset + 32 <= self.rom.bytes().len() {
                let slice = slice_lzss(self.rom, offset);
                if let Ok(raw) = decompress_lzss(slice) {
                    if let Ok(p) = PaletteData::from_bgr555(&raw) {
                        return p;
                    }
                }
            }
        }
        PaletteData::default()
    }

    /// Derive a shiny palette for an overworld sprite from the battle sprite palette.
    fn resolve_shiny_palette(
        &self,
        name: &str,
        species_names: &[String],
    ) -> Option<PaletteData> {
        let lower = name.to_ascii_lowercase();
        let species_idx = species_names.iter().position(|n| n == &lower)?;
        let start_sym = self.symbols.get("Start")?;
        let start = start_sym.address;
        let table_sym = self.symbols.get("gMonShinyPaletteTable")?;
        let table_offset = (table_sym.address - start) as usize;
        let elem_offset = table_offset + species_idx * 8;
        let b = &self.rom.bytes()[elem_offset..elem_offset + 8];
        let data_ptr = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        if data_ptr == 0 {
            return None;
        }
        let raw = self.read_field(data_ptr, POKEMON_PALETTE_BYTES, true).ok()?;
        PaletteData::from_bgr555(&raw).ok()
    }
}

/// Tileset-related byte lengths read from the symbol table.
#[derive(Default, Debug, Clone, Copy)]
pub struct TilesetInfo {
    /// Length of tile graphics data in bytes (0 if unknown).
    pub tiles_length: usize,
    /// Length of palette data in bytes (0 if unknown).
    pub palettes_length: usize,
}

fn find_primary<'a>(
    pairs: &'a BTreeMap<String, Vec<String>>,
    secondary: &'a str,
) -> Option<&'a str> {
    if pairs.contains_key(secondary) {
        return Some(secondary);
    }
    for (primary, secondaries) in pairs {
        if secondaries.iter().any(|s| s == secondary) {
            return Some(primary.as_str());
        }
    }
    None
}

fn slice_lzss(rom: &Rom, offset: usize) -> &[u8] {
    let end = (offset + MAX_LZSS_READ).min(rom.bytes().len());
    &rom.bytes()[offset..end]
}

fn species_index_of(name: &str, species_names: &[String]) -> u16 {
    species_names
        .iter()
        .position(|n| n == name)
        .map_or(0, |i| i as u16)
}

fn derive_species_form(suffix: &str, species_names: &[String]) -> Option<(String, String)> {
    let lower = suffix.to_ascii_lowercase();
    let norm_suffix: String = lower.chars().filter(|c| c.is_alphanumeric()).collect();
    if norm_suffix.is_empty() {
        return None;
    }
    let mut species_match: Option<&str> = None;
    for name in species_names {
        if name.is_empty() {
            continue;
        }
        let norm: String = name.chars().filter(|c| c.is_alphanumeric()).collect();
        if norm == norm_suffix {
            if suffix == name {
                return None;
            }
            if lower == *name {
                return None;
            }
            if lower.starts_with(name) && lower.len() > name.len() {
                return Some((name.clone(), suffix[name.len()..].to_owned()));
            }
            species_match = Some(name.as_str());
        } else if !norm.is_empty()
            && norm_suffix.starts_with(&norm)
            && norm_suffix.len() > norm.len()
        {
            species_match = Some(name.as_str());
        }
    }
    if let Some(species) = species_match {
        let norm_species: String = species.chars().filter(|c| c.is_alphanumeric()).collect();
        if let Some(idx) = lower.find(species) {
            let after = idx + species.len();
            if after < lower.len() {
                return Some((species.to_owned(), suffix[after..].to_owned()));
            }
        }
        if let Some(pos) = norm_suffix.find(&norm_species) {
            let alnum_before = lower.chars().take_while(|c| c.is_alphanumeric()).count();
            if pos + norm_species.len() < alnum_before {
                return Some((
                    species.to_owned(),
                    suffix[pos + norm_species.len()..].to_owned(),
                ));
            }
        }
    }
    None
}

/// Decode a Pokemon text string (up to 11 bytes) into a lowercase name.
fn decode_species_name(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len());
    for &b in raw {
        if let Some(ch) = pokemon_char(b) {
            out.push_str(ch);
        }
    }
    out.to_ascii_lowercase()
}

/// Map a single Pokemon GBA character byte to its ASCII rendering.
#[must_use]
pub fn pokemon_char(b: u8) -> Option<&'static str> {
    Some(match b {
        0x00 | 0xFF => "",
        0xBB => "A",
        0xBC => "B",
        0xBD => "C",
        0xBE => "D",
        0xBF => "E",
        0xC0 => "F",
        0xC1 => "G",
        0xC2 => "H",
        0xC3 => "I",
        0xC4 => "J",
        0xC5 => "K",
        0xC6 => "L",
        0xC7 => "M",
        0xC8 => "N",
        0xC9 => "O",
        0xCA => "P",
        0xCB => "Q",
        0xCC => "R",
        0xCD => "S",
        0xCE => "T",
        0xCF => "U",
        0xD0 => "V",
        0xD1 => "W",
        0xD2 => "X",
        0xD3 => "Y",
        0xD4 => "Z",
        0xA1 => "0",
        0xA2 => "1",
        0xA3 => "2",
        0xA4 => "3",
        0xA5 => "4",
        0xA6 => "5",
        0xA7 => "6",
        0xA8 => "7",
        0xA9 => "8",
        0xAA => "9",
        0xAB => "!",
        0xAC => "?",
        0xAD => " ",
        0xAE => "-",
        0xB5 => "M",
        0xB6 => "F",
        0xE0 => "'",
        0xE1 => "D",
        _ => return None,
    })
}

/// Rearrange tile data from GBA quadrant layout to row-major order.
///
/// The GBA OAM stores tiles for sprites larger than 64x64 in quadrants:
///   - Top-left: tiles 0..N
///   - Top-right: tiles N..2N
///   - Bottom-left: tiles 2N..3N
///   - Bottom-right: tiles 3N..4N
///
/// where N = (width_tiles/2) * (height_tiles/2).
///
/// This function rearranges them into row-major order so that tile index `i`
/// corresponds to column `i % width_tiles` and row `i / width_tiles`.
///
/// For sprites ≤ 64x64, the data is already in row-major order and this
/// function returns a copy unchanged.
fn rearrange_quadrant_tiles(data: &[u8], width_tiles: u16, height_tiles: u16) -> Vec<u8> {
    const TILE_BYTES: usize = 32; // 4bpp: 32 bytes per 8x8 tile
    let total_tiles = width_tiles as usize * height_tiles as usize;
    let expected = total_tiles * TILE_BYTES;

    // Only rearrange for the 2×2 quadrant layout used by 128x64 sprites.
    // Sprites without -mwidth (like SS Tidal) use default row-major order.
    if width_tiles <= 8 || height_tiles < 8 || data.len() < expected {
        return data[..data.len().min(expected)].to_vec();
    }

    let half_w = width_tiles as usize / 2;
    let half_h = height_tiles as usize / 2;
    let quadrant_tiles = half_w * half_h;

    let mut out = vec![0u8; expected];

    for qy in 0..2 {
        for qx in 0..2 {
            let src_q = (qy * 2 + qx) * quadrant_tiles;
            for ty in 0..half_h {
                for tx in 0..half_w {
                    let src_idx = src_q + ty * half_w + tx;
                    let dst_idx = (qy * half_h + ty) * width_tiles as usize + (qx * half_w + tx);
                    let src_byte = src_idx * TILE_BYTES;
                    let dst_byte = dst_idx * TILE_BYTES;
                    if src_byte + TILE_BYTES <= data.len() && dst_byte + TILE_BYTES <= out.len() {
                        out[dst_byte..dst_byte + TILE_BYTES]
                            .copy_from_slice(&data[src_byte..src_byte + TILE_BYTES]);
                    }
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_species_name_pikachu() {
        // PIKACHU = 0xCA 0xC3 0xC5 0xBB 0xBD 0xC2 0xCF 0xFF ... + 0x00 padding
        let raw = [
            0xCA, 0xC3, 0xC5, 0xBB, 0xBD, 0xC2, 0xCF, 0xFF, 0x00, 0x00, 0x00,
        ];
        let name = decode_species_name(&raw);
        assert_eq!(name, "pikachu");
    }

    #[test]
    fn decode_species_name_terminator_stops() {
        let raw = [
            0xCA, 0xC3, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(decode_species_name(&raw), "pi");
    }

    #[test]
    fn derive_species_form_known() {
        let names = vec!["unown".to_owned(), "pikachu".to_owned()];
        let (species, form) = derive_species_form("UnownB", &names).unwrap();
        assert_eq!(species, "unown");
        assert_eq!(form, "B");
    }

    #[test]
    fn derive_species_form_returns_none_for_base() {
        let names = vec!["pikachu".to_owned()];
        assert!(derive_species_form("Pikachu", &names).is_none());
    }
}
