//! ROM loading and game identification.

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// Default GBA ROM base address.
pub const DEFAULT_ROM_BASE_ADDRESS: u32 = 0x0800_0000;

/// Byte offset of the 4-byte game code within the ROM header.
pub const GAME_CODE_OFFSET: usize = 0xAC;

/// Length of the game code in bytes.
pub const GAME_CODE_LENGTH: usize = 4;

/// A fully loaded GBA Pokemon ROM.
#[derive(Debug, Clone)]
pub struct Rom {
    bytes: Vec<u8>,
    game: Game,
    base_address: u32,
}

impl Rom {
    /// Load a ROM from a file on disk and identify its game.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the file cannot be read. See [`Self::from_bytes`]
    /// for additional error conditions.
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_bytes(bytes)
    }

    /// Construct a ROM from already-loaded bytes and identify its game.
    ///
    /// # Errors
    ///
    /// Returns [`Error::RomTooSmall`] if `bytes` is smaller than the header,
    /// or [`Error::InvalidGameCode`] if the game code does not match a known
    /// game.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() < GAME_CODE_OFFSET + GAME_CODE_LENGTH {
            return Err(Error::RomTooSmall {
                size: bytes.len(),
                needed: GAME_CODE_OFFSET + GAME_CODE_LENGTH,
            });
        }
        let code = [
            bytes[GAME_CODE_OFFSET],
            bytes[GAME_CODE_OFFSET + 1],
            bytes[GAME_CODE_OFFSET + 2],
            bytes[GAME_CODE_OFFSET + 3],
        ];
        let game = Game::from_code(code).ok_or(Error::InvalidGameCode { code })?;
        Ok(Self {
            bytes,
            game,
            base_address: DEFAULT_ROM_BASE_ADDRESS,
        })
    }

    /// Returns the identified game.
    #[must_use]
    pub const fn game(&self) -> Game {
        self.game
    }

    /// Returns the raw ROM bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the GBA base address (default `0x0800_0000`).
    #[must_use]
    pub const fn base_address(&self) -> u32 {
        self.base_address
    }

    /// Convert a ROM address to a file offset.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfRange`] if the resulting offset is outside the
    /// loaded bytes.
    pub fn offset_of(&self, address: u32) -> Result<usize> {
        let offset = address
            .checked_sub(self.base_address)
            .ok_or(Error::OutOfRange {
                offset: address,
                size: self.bytes.len() as u32,
            })?;
        let offset = offset as usize;
        if offset >= self.bytes.len() {
            return Err(Error::OutOfRange {
                offset: address,
                size: self.bytes.len() as u32,
            });
        }
        Ok(offset)
    }

    /// Borrow a slice of `len` bytes starting at `address`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfRange`] if the range falls outside the ROM.
    pub fn slice_at(&self, address: u32, len: usize) -> Result<&[u8]> {
        let start = self.offset_of(address)?;
        let end = start.checked_add(len).ok_or(Error::OutOfRange {
            offset: address.wrapping_add(len as u32),
            size: self.bytes.len() as u32,
        })?;
        if end > self.bytes.len() {
            return Err(Error::OutOfRange {
                offset: address,
                size: self.bytes.len() as u32,
            });
        }
        Ok(&self.bytes[start..end])
    }

    /// Returns the SHA-256 hash of the ROM bytes, as lowercase hex.
    #[must_use]
    pub fn sha256(&self) -> String {
        let mut out = String::with_capacity(64);
        for b in Sha256::digest(&self.bytes) {
            use std::fmt::Write;
            let _ = write!(out, "{b:02x}");
        }
        out
    }
}

/// One of the five supported GBA Pokemon games.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Game {
    /// Pokemon Emerald.
    Emerald,
    /// Pokemon FireRed.
    FireRed,
    /// Pokemon LeafGreen.
    LeafGreen,
    /// Pokemon Ruby.
    Ruby,
    /// Pokemon Sapphire.
    Sapphire,
}

impl Game {
    /// All supported games.
    pub const ALL: &'static [Self] = &[
        Self::Emerald,
        Self::FireRed,
        Self::LeafGreen,
        Self::Ruby,
        Self::Sapphire,
    ];

    /// Look up a game by its 4-byte ROM code.
    #[must_use]
    pub const fn from_code(code: [u8; 4]) -> Option<Self> {
        match &code {
            b"BPEE" => Some(Self::Emerald),
            b"BPRE" => Some(Self::FireRed),
            b"BPGE" => Some(Self::LeafGreen),
            b"AXVE" => Some(Self::Ruby),
            b"AXPE" => Some(Self::Sapphire),
            _ => None,
        }
    }

    /// Look up a game by its short or full name (case-insensitive).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let normalized: String = name
            .to_ascii_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        match normalized.as_str() {
            "emerald" | "pokemonemerald" => Some(Self::Emerald),
            "firered" | "pokemonfirered" => Some(Self::FireRed),
            "leafgreen" | "pokemonleafgreen" => Some(Self::LeafGreen),
            "ruby" | "pokemonruby" => Some(Self::Ruby),
            "sapphire" | "pokemonsapphire" => Some(Self::Sapphire),
            _ => None,
        }
    }

    /// 4-byte ROM code for this game.
    #[must_use]
    pub const fn code(self) -> [u8; 4] {
        match self {
            Self::Emerald => *b"BPEE",
            Self::FireRed => *b"BPRE",
            Self::LeafGreen => *b"BPGE",
            Self::Ruby => *b"AXVE",
            Self::Sapphire => *b"AXPE",
        }
    }

    /// Full display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Emerald => "Pokemon Emerald",
            Self::FireRed => "Pokemon FireRed",
            Self::LeafGreen => "Pokemon LeafGreen",
            Self::Ruby => "Pokemon Ruby",
            Self::Sapphire => "Pokemon Sapphire",
        }
    }

    /// Short identifier used in CLI flags and the `.sym` URL.
    #[must_use]
    pub const fn short_name(self) -> &'static str {
        match self {
            Self::Emerald => "emerald",
            Self::FireRed => "firered",
            Self::LeafGreen => "leafgreen",
            Self::Ruby => "ruby",
            Self::Sapphire => "sapphire",
        }
    }

    /// pret disassembly repository name on GitHub.
    #[must_use]
    pub const fn sym_repo(self) -> &'static str {
        match self {
            Self::Emerald => "pokeemerald",
            Self::FireRed | Self::LeafGreen => "pokefirered",
            Self::Ruby | Self::Sapphire => "pokeruby",
        }
    }

    /// Default symbol file name for this game.
    #[must_use]
    pub const fn default_sym_file(self) -> &'static str {
        match self {
            Self::Emerald => "pokeemerald.sym",
            Self::FireRed | Self::LeafGreen => "pokefirered.sym",
            Self::Ruby | Self::Sapphire => "pokeruby.sym",
        }
    }

    /// URL to download the default `.sym` file for this game.
    #[must_use]
    pub fn sym_url(self) -> String {
        format!(
            "https://cdn.jsdelivr.net/gh/pret/{repo}@symbols/{file}",
            repo = self.sym_repo(),
            file = self.default_sym_file(),
        )
    }

    /// URL to download a revision-specific `.sym` file for this game.
    #[must_use]
    pub fn sym_url_for_file(self, file: &str) -> String {
        format!(
            "https://cdn.jsdelivr.net/gh/pret/{repo}@symbols/{file}",
            repo = self.sym_repo(),
            file = file,
        )
    }

    /// Revision `.sym` filenames to try when the default does not match.
    ///
    /// Returns an empty slice for games without known revisions.
    #[must_use]
    pub fn revision_sym_files(self) -> &'static [&'static str] {
        match self {
            Self::Ruby => &["pokeruby_rev1.sym", "pokeruby_rev2.sym"],
            Self::Sapphire => &["pokesapphire_rev1.sym", "pokesapphire_rev2.sym"],
            Self::FireRed => &["pokefirered_rev1.sym"],
            Self::LeafGreen => &["pokeleafgreen_rev1.sym"],
            Self::Emerald => &[],
        }
    }

    /// Number of tiles in this game's primary tileset.
    #[must_use]
    pub const fn primary_tile_count(self) -> u16 {
        match self {
            Self::FireRed | Self::LeafGreen => 0x280,
            _ => 0x200,
        }
    }
}

/// Per-game configuration that drives symbol download and extraction.
#[derive(Debug, Clone)]
pub struct GameProfile {
    /// The game this profile describes.
    pub game: Game,
    /// SHA-256 → symbol filename overrides for ROM revisions.
    pub revisions: Vec<(String, &'static str)>,
}

impl GameProfile {
    /// Return the canonical `.sym` URL for a known SHA-256, if any.
    #[must_use]
    pub fn sym_url_for(&self, sha256: &str) -> Option<String> {
        let (_, filename) = self.revisions.iter().find(|(s, _)| s == sha256)?;
        Some(format!(
            "https://cdn.jsdelivr.net/gh/pret/{repo}@symbols/{file}",
            repo = self.game.sym_repo(),
            file = filename,
        ))
    }

    /// Default `.sym` URL when the SHA-256 is not in the revision table.
    #[must_use]
    pub fn default_sym_url(&self) -> String {
        format!(
            "https://cdn.jsdelivr.net/gh/pret/{repo}@symbols/{file}",
            repo = self.game.sym_repo(),
            file = self.game.default_sym_file(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom_with_code(code: &[u8; 4]) -> Vec<u8> {
        let mut bytes = vec![0u8; 0xB0];
        bytes[GAME_CODE_OFFSET..GAME_CODE_OFFSET + GAME_CODE_LENGTH].copy_from_slice(code);
        bytes
    }

    #[test]
    fn identifies_emerald() {
        let rom = Rom::from_bytes(rom_with_code(b"BPEE")).unwrap();
        assert_eq!(rom.game(), Game::Emerald);
    }

    #[test]
    fn identifies_firered() {
        let rom = Rom::from_bytes(rom_with_code(b"BPRE")).unwrap();
        assert_eq!(rom.game(), Game::FireRed);
    }

    #[test]
    fn rejects_unknown_game_code() {
        let rom = Rom::from_bytes(rom_with_code(b"XXXX"));
        assert!(rom.is_err());
    }

    #[test]
    fn rejects_too_small_rom() {
        let rom = Rom::from_bytes(vec![0u8; 0x10]);
        assert!(matches!(rom, Err(Error::RomTooSmall { .. })));
    }

    #[test]
    fn offset_of_within_bounds() {
        let rom = Rom::from_bytes(rom_with_code(b"BPEE")).unwrap();
        assert_eq!(rom.offset_of(0x0800_0000).unwrap(), 0);
    }

    #[test]
    fn offset_of_out_of_range() {
        let rom = Rom::from_bytes(rom_with_code(b"BPEE")).unwrap();
        assert!(matches!(
            rom.offset_of(0x0900_0000),
            Err(Error::OutOfRange { .. })
        ));
    }

    #[test]
    fn slice_at_within_bounds() {
        let rom = Rom::from_bytes(rom_with_code(b"BPEE")).unwrap();
        let addr = DEFAULT_ROM_BASE_ADDRESS + GAME_CODE_OFFSET as u32;
        let slice = rom.slice_at(addr, 4).unwrap();
        assert_eq!(slice, b"BPEE");
    }

    #[test]
    fn game_from_name_short() {
        assert_eq!(Game::from_name("emerald"), Some(Game::Emerald));
        assert_eq!(Game::from_name("FIRERED"), Some(Game::FireRed));
    }

    #[test]
    fn game_from_name_full() {
        assert_eq!(Game::from_name("pokemon ruby"), Some(Game::Ruby));
    }

    #[test]
    fn game_from_name_unknown() {
        assert_eq!(Game::from_name("Crystal"), None);
    }

    #[test]
    fn game_code_roundtrip() {
        for &game in Game::ALL {
            let code = game.code();
            assert_eq!(Game::from_code(code), Some(game));
        }
    }
}
