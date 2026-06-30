//! Symbol table parser for pret `.sym` files.

use std::io::{BufRead, BufReader, Read};

use crate::error::{Error, Result};

/// One entry in a parsed `.sym` file.
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

impl SymbolTable {
    /// Parse a symbol table from any `Read` source.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the reader fails, or [`Error::InvalidGameCode`]
    /// is not used here; parsing only fails if the data is malformed in ways
    /// that prevent producing a valid table.
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
            let address = parse_hex(addr_str).ok_or(Error::MalformedSymbol {
                detail: "address field is not valid hex",
            })?;
            let length: u32 = u32::from_str_radix(len_str, 16).map_err(|_| {
                Error::MalformedSymbol {
                    detail: "length field is not valid hex",
                }
            })?;
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
            });
        }

        // Fill in zero-length entries using the next symbol's address.
        for i in 0..entries.len() {
            if entries[i].length == 0 {
                if let Some(next) = entries.get(i + 1) {
                    if next.address > entries[i].address {
                        entries[i].length = next.address - entries[i].address;
                    }
                }
            }
        }

        let mut by_name = std::collections::HashMap::with_capacity(entries.len());
        let mut by_address = std::collections::HashMap::with_capacity(entries.len());
        for (i, sym) in entries.iter().enumerate() {
            by_name.insert(sym.name.clone(), i);
            by_address.insert(sym.address, i);
        }

        Ok(Self {
            entries,
            by_name,
            by_address,
        })
    }

    /// Parse a symbol table from raw text.
    ///
    /// # Errors
    ///
    /// See [`Self::parse`].
    pub fn from_text(text: &str) -> Result<Self> {
        Self::parse(text.as_bytes())
    }

    /// Read a symbol table from a file on disk.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the file cannot be read.
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = std::fs::File::open(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(file)
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

fn parse_hex(s: &str) -> Option<u32> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    u32::from_str_radix(s, 16).ok()
}

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
}
