//! Symbol table parser for pret `.sym` files and bundled TOML.

use std::io::{BufRead, BufReader, Read};

use serde::Deserialize;

use crate::error::{Error, Result};

/// One entry in a parsed symbol table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// Name of the symbol.
    pub name: String,
    /// Address of the symbol.
    pub address: u32,
    /// Length of the symbol in bytes, or 0 if unknown.
    pub length: u32,
    /// Whether the symbol is global or local.
    pub scope: Scope,
    /// Optional human-readable asset label (derived from the symbol name
    /// or curated).  `None` when the asset name can be inferred at use
    /// site (e.g. by stripping a known prefix).
    pub asset: Option<String>,
}

/// Scope of a symbol.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Scope {
    /// Global symbol (marked `g` in the `.sym` file).
    /// These are accessible from any compilation unit in the ROM.
    Global,
    /// Local symbol (marked `l` in the `.sym` file).
    /// These are only visible within the compilation unit that defined them.
    Local,
}

/// In-memory symbol table with O(1) name and address lookups.
#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    entries: Vec<Symbol>,
    by_name: std::collections::HashMap<String, usize>,
    by_address: std::collections::HashMap<u32, usize>,
}

// ---------------------------------------------------------------------------
// .sym parser (legacy, kept for --sym-file and the generator)
// ---------------------------------------------------------------------------

impl SymbolTable {
    /// Parse a symbol table from any `Read` source.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the reader fails.
    pub fn parse(reader: impl Read) -> Result<Self> {
        let mut entries = Vec::new();
        for line in BufReader::new(reader).lines() {
            let line = line.map_err(|source| Error::Io {
                path: std::path::PathBuf::new(),
                source,
            })?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let mut parts = trimmed.split_whitespace();
            let Some(addr_str) = parts.next() else {
                continue;
            };
            let Some(scope_str) = parts.next() else {
                continue;
            };
            let Some(len_str) = parts.next() else {
                continue;
            };
            let name = parts.collect::<Vec<_>>().join(" ");
            if name.is_empty() {
                continue;
            }
            let address = parse_hex(addr_str).ok_or(Error::SymbolNotFound { name: "address" })?;
            let length: u32 = u32::from_str_radix(len_str, 16)
                .map_err(|_| Error::SymbolNotFound { name: "length" })?;
            let scope = match scope_str {
                "g" => Scope::Global,
                "l" => Scope::Local,
                _ => continue,
            };
            entries.push(Symbol {
                name,
                address,
                length,
                scope,
                asset: None,
            });
        }
        fill_lengths(&mut entries);
        Ok(Self::from_entries(entries))
    }

    /// Parse a symbol table from raw `.sym` text.
    ///
    /// # Errors
    ///
    /// See [`Self::parse`].
    pub fn from_text(text: &str) -> Result<Self> {
        Self::parse(text.as_bytes())
    }

    /// Read a symbol table from a file on disk. TOML is preferred,
    /// with automatic fallback to legacy `.sym`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the file cannot be read.
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml(&text).or_else(|_| Self::from_text(&text))
    }
}

// ---------------------------------------------------------------------------
// TOML parser
// ---------------------------------------------------------------------------

/// Intermediate representation for the TOML symbol-file format.
#[derive(Default, Deserialize)]
#[allow(dead_code)]
struct SymbolFile {
    game: Option<String>,
    revision: Option<String>,
    #[serde(default)]
    start: Option<u32>,
    #[serde(default)]
    tilesets: Vec<FlatEntry>,
    #[serde(default)]
    metatiles: Vec<FlatEntry>,
    #[serde(default)]
    pokemon_tables: Vec<FlatEntry>,
    #[serde(default)]
    field_sprites: Vec<FlatEntry>,
    #[serde(default)]
    field_sprite_palettes: Vec<FlatEntry>,
    #[serde(default)]
    tables: Vec<FlatEntry>,
    #[serde(default)]
    pokemon: std::collections::HashMap<String, PokemonGroup>,
}

#[derive(Deserialize)]
struct FlatEntry {
    name: String,
    offset: u32,
    #[serde(default)]
    length: u32,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    asset: Option<String>,
    /// Offset for the associated `gTilesetTiles_{name}` symbol (tileset
    /// tiles). Only meaningful in `tilesets` entries.
    #[serde(default)]
    tiles: Option<u32>,
    /// Offset for the associated `gTilesetPalettes_{name}` symbol (tileset
    /// palettes). Only meaningful in `tilesets` entries.
    #[serde(default)]
    palettes: Option<u32>,
}

#[derive(Default, Deserialize)]
struct PokemonGroup {
    #[serde(default)]
    asset: Option<String>,
    front: Option<Component>,
    back: Option<Component>,
    palette: Option<Component>,
    shiny_palette: Option<Component>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Component {
    Offset(u32),
    Detailed {
        #[serde(default)]
        name: Option<String>,
        offset: u32,
        #[serde(default)]
        length: u32,
        #[serde(default)]
        scope: Option<String>,
    },
}

/// Prefixes used to reconstruct pokemon-component symbol names.
const POKEMON_COMPONENT_PREFIXES: [(&str, &str); 4] = [
    ("front", "gMonFrontPic_"),
    ("back", "gMonBackPic_"),
    ("palette", "gMonPalette_"),
    ("shiny_palette", "gMonShinyPalette_"),
];

impl SymbolTable {
    /// Parse a grouped TOML symbol table.
    ///
    /// Offsets are file-relative: the global GBA address is computed as
    /// `start + offset`, where `start` defaults to `0x08000000`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SymbolParse`] when the TOML is malformed.
    pub fn from_toml(text: &str) -> Result<Self> {
        let file: SymbolFile = toml::from_str(text)?;

        let start = file.start.unwrap_or(0x0800_0000);
        let mut entries: Vec<Symbol> = Vec::new();

        // Flat sections (tilesets first — may spawn extra helper symbols).
        for raw in &file.tilesets {
            let bare_name = raw
                .name
                .strip_prefix("gTileset_")
                .unwrap_or(&raw.name)
                .to_owned();
            entries.push(flat_to_symbol(raw, start));
            if let Some(tiles_off) = raw.tiles {
                entries.push(Symbol {
                    name: format!("gTilesetTiles_{bare_name}"),
                    address: start + tiles_off,
                    length: 0,
                    scope: Scope::Global,
                    asset: None,
                });
            }
            if let Some(palettes_off) = raw.palettes {
                entries.push(Symbol {
                    name: format!("gTilesetPalettes_{bare_name}"),
                    address: start + palettes_off,
                    length: 0,
                    scope: Scope::Global,
                    asset: None,
                });
            }
        }
        for raw in file
            .metatiles
            .iter()
            .chain(file.pokemon_tables.iter())
            .chain(file.field_sprites.iter())
            .chain(file.field_sprite_palettes.iter())
            .chain(file.tables.iter())
        {
            entries.push(flat_to_symbol(raw, start));
        }

        // Pokemon groups.
        for (key, group) in &file.pokemon {
            let group_asset = group.asset.clone();
            // One entry per present component.
            for &(field, prefix) in &POKEMON_COMPONENT_PREFIXES {
                let comp = match field {
                    "front" => &group.front,
                    "back" => &group.back,
                    "palette" => &group.palette,
                    "shiny_palette" => &group.shiny_palette,
                    _ => continue,
                };
                let Some(comp) = comp else { continue };
                let (name, offset, length, scope_str) = match comp {
                    Component::Offset(o) => {
                        let n = format!("{prefix}{key}");
                        (n, *o, 0u32, None)
                    }
                    Component::Detailed {
                        name,
                        offset,
                        length,
                        scope,
                    } => {
                        let n = name.clone().unwrap_or_else(|| format!("{prefix}{key}"));
                        (n, *offset, *length, scope.clone())
                    }
                };
                let scope = parse_scope(scope_str.as_deref());
                entries.push(Symbol {
                    name,
                    address: start + offset,
                    length,
                    scope,
                    asset: group_asset.clone(),
                });
            }
        }

        fill_lengths(&mut entries);
        Ok(Self::from_entries(entries))
    }

    /// Resolve a symbol table by trying each bundled TOML for the game and
    /// returning the first that validates against the given ROM.
    ///
    /// Falls back to the first bundled table if none validates.
    ///
    /// # Errors
    ///
    /// Returns [`Error::SymbolParse`] if no bundled TOML can be parsed.
    pub fn resolve_for_rom(rom: &crate::Rom) -> Result<Self> {
        let tables = rom.game().bundled_symbol_tables();
        for toml in tables {
            match Self::from_toml(toml) {
                Ok(table) => {
                    if crate::Extractor::new(rom, &table).validate() {
                        return Ok(table);
                    }
                }
                Err(e) => {
                    log::debug!("bundled TOML parse error: {e}");
                }
            }
        }
        // Fall back to the first table (may not validate but still usable).
        Self::from_toml(tables.first().copied().unwrap_or(""))
    }

    fn from_entries(mut entries: Vec<Symbol>) -> Self {
        entries.sort_by_key(|s| s.address);
        let mut by_name = std::collections::HashMap::with_capacity(entries.len());
        let mut by_address = std::collections::HashMap::with_capacity(entries.len());
        for (i, sym) in entries.iter().enumerate() {
            by_name.insert(sym.name.clone(), i);
            by_address.insert(sym.address, i);
        }
        Self {
            entries,
            by_name,
            by_address,
        }
    }

    /// Look up a symbol by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.by_name.get(name).map(|&i| &self.entries[i])
    }

    /// Look up a symbol by its address.
    #[must_use]
    pub fn by_address(&self, address: u32) -> Option<&Symbol> {
        self.by_address.get(&address).map(|&i| &self.entries[i])
    }

    /// Returns the number of symbols in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the table has no symbols.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all symbols.
    pub fn iter(&self) -> impl Iterator<Item = &Symbol> {
        self.entries.iter()
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn flat_to_symbol(raw: &FlatEntry, start: u32) -> Symbol {
    Symbol {
        name: raw.name.clone(),
        address: start + raw.offset,
        length: raw.length,
        scope: parse_scope(raw.scope.as_deref()),
        asset: raw.asset.clone(),
    }
}

fn parse_scope(s: Option<&str>) -> Scope {
    match s {
        Some("local") => Scope::Local,
        _ => Scope::Global,
    }
}

/// Fill zero-length entries using the next symbol's address.
fn fill_lengths(entries: &mut [Symbol]) {
    entries.sort_by_key(|s| s.address);
    for i in 0..entries.len() {
        if entries[i].length == 0 {
            if let Some(next) = entries.get(i + 1) {
                if next.address > entries[i].address {
                    entries[i].length = next.address - entries[i].address;
                }
            }
        }
    }
}

fn parse_hex(s: &str) -> Option<u32> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    u32::from_str_radix(s, 16).ok()
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
08000000 g 00000010 Start
08000010 g 00000000 gMonFrontPicTable
08000100 g 00000020 gTileset_Overworld
";

    #[test]
    fn parse_basic_symbols() {
        let table = SymbolTable::from_text(SAMPLE).unwrap();
        assert_eq!(table.len(), 3);
        let s = table.get("Start").unwrap();
        assert_eq!(s.address, 0x0800_0000);
        assert_eq!(s.length, 0x10);
        assert_eq!(s.scope, Scope::Global);
    }

    #[test]
    fn calculates_length_from_next_symbol() {
        let table = SymbolTable::from_text(SAMPLE).unwrap();
        let s = table.get("gMonFrontPicTable").unwrap();
        assert_eq!(s.length, 0xF0);
    }

    #[test]
    fn by_address_lookup() {
        let table = SymbolTable::from_text(SAMPLE).unwrap();
        let s = table.by_address(0x0800_0100).unwrap();
        assert_eq!(s.name, "gTileset_Overworld");
    }

    #[test]
    fn local_scope() {
        let table = SymbolTable::from_text("08000000 l 00000004 .local_sym\n").unwrap();
        assert_eq!(table.get(".local_sym").unwrap().scope, Scope::Local);
    }

    #[test]
    fn empty_input_yields_empty_table() {
        let table = SymbolTable::from_text("").unwrap();
        assert!(table.is_empty());
    }

    #[test]
    fn from_toml_flat_sections() {
        let toml = r#"
game = "emerald"
revision = "default"

tilesets = [
  { name = "gTileset_General", offset = 0x1234, length = 0x20 },
]
tables = [
  { name = "Start", offset = 0x0, length = 0x10 },
  { name = "sSpeciesToNationalPokedexNum", offset = 0x14000, length = 0x680, scope = "local" },
]
"#;
        let table = SymbolTable::from_toml(toml).unwrap();
        assert_eq!(table.len(), 3);
        let s = table.get("Start").unwrap();
        assert_eq!(s.address, 0x0800_0000);
        assert_eq!(s.length, 0x10);
        assert_eq!(s.scope, Scope::Global);
        let s = table.get("gTileset_General").unwrap();
        assert_eq!(s.address, 0x0800_1234);
        assert_eq!(s.length, 0x20);
        let s = table.get("sSpeciesToNationalPokedexNum").unwrap();
        assert_eq!(s.scope, Scope::Local);
    }

    #[test]
    fn from_toml_pokemon_component_offsets() {
        let toml = r#"
game = "emerald"
revision = "default"
tables = []
tilesets = []
metatiles = []
pokemon_tables = []

[pokemon.Pikachu]
front = 0xB000
back = 0xB200
palette = 0xB400
shiny_palette = 0xB600
"#;
        let table = SymbolTable::from_toml(toml).unwrap();
        assert_eq!(table.len(), 4);
        let s = table.get("gMonFrontPic_Pikachu").unwrap();
        assert_eq!(s.address, 0x0800_B000);
        let s = table.get("gMonBackPic_Pikachu").unwrap();
        assert_eq!(s.address, 0x0800_B200);
    }

    #[test]
    fn from_toml_length_fill() {
        let toml = r#"
tables = [ { name = "A", offset = 0x10 }, { name = "B", offset = 0x20 } ]
tilesets = []
metatiles = []
pokemon_tables = []
"#;
        let table = SymbolTable::from_toml(toml).unwrap();
        assert_eq!(table.get("A").unwrap().length, 0x10);
    }

    #[test]
    fn bundled_tomls_are_parseable() {
        for &game in crate::Game::ALL {
            for toml in game.bundled_symbol_tables() {
                let table = SymbolTable::from_toml(toml).unwrap();
                assert!(table.get("Start").is_some(), "{game:?} missing Start");
                assert!(
                    table.get("gMapLayouts").is_some(),
                    "{game:?} missing gMapLayouts"
                );
            }
        }
    }
}
