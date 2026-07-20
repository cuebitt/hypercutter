//! Error types for the hypercutter library.

use std::path::PathBuf;

/// Result alias for the library.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur when extracting data from a Pokemon ROM.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// The ROM is too small to contain a valid header.
    #[error("ROM too small: {size} bytes, need at least {needed}")]
    RomTooSmall {
        /// Number of bytes in the input.
        size: usize,
        /// Minimum bytes required for a valid header.
        needed: usize,
    },

    /// The game code at the expected offset does not match a supported game.
    #[error("invalid game code: {code:?}")]
    InvalidGameCode {
        /// The unrecognized 4-byte game code.
        code: [u8; 4],
    },

    /// The given game name does not match any known game.
    #[error("unknown game: {name}")]
    UnknownGame {
        /// The supplied game name.
        name: String,
    },

    /// A required symbol was not present in the .sym file.
    #[error("symbol {name:?} not found in .sym file")]
    SymbolNotFound {
        /// The missing symbol name.
        name: &'static str,
    },

    /// A ROM address fell outside the loaded bytes.
    #[error("offset {offset:#x} out of range (ROM size {size:#x})")]
    OutOfRange {
        /// The address that was out of range.
        offset: u32,
        /// Total size of the loaded ROM in bytes.
        size: u32,
    },

    /// The LZSS header byte was not 0x10 or 0x11.
    #[error("invalid LZSS magic byte: 0x{magic:02x}")]
    InvalidLzssMagic {
        /// The unexpected magic byte.
        magic: u8,
    },

    /// LZSS decompression failed.
    #[error("LZSS decompression failed: {message}")]
    Decompression {
        /// Description of the failure.
        message: String,
    },

    /// The given tileset name was not found.
    #[error("unknown tileset: {0}")]
    UnknownTileset(#[doc = "The supplied tileset name."] String),

    /// The given species id was outside the known range.
    #[error("unknown species id: {0}")]
    UnknownSpecies(#[doc = "The supplied species id."] u16),

    /// An I/O error occurred while reading or writing.
    #[error("I/O error at {path}")]
    Io {
        /// Path of the file involved in the failed I/O.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// A PNG encoding error.
    #[error("PNG encoding error: {message}")]
    Png {
        /// Description of the failure.
        message: String,
    },

    /// Failed to parse symbol data (TOML or other format).
    #[error("symbol table parse error: {message}")]
    SymbolParse {
        /// Description of the failure.
        message: String,
    },
}

impl From<std::io::Error> for Error {
    fn from(source: std::io::Error) -> Self {
        Self::Io {
            path: PathBuf::new(),
            source,
        }
    }
}

impl From<png::EncodingError> for Error {
    fn from(source: png::EncodingError) -> Self {
        Self::Png {
            message: source.to_string(),
        }
    }
}

impl From<toml::de::Error> for Error {
    fn from(source: toml::de::Error) -> Self {
        Self::SymbolParse {
            message: source.to_string(),
        }
    }
}

impl From<binrw::Error> for Error {
    fn from(source: binrw::Error) -> Self {
        Self::Io {
            path: std::path::PathBuf::from("<binrw>"),
            source: std::io::Error::other(source.to_string()),
        }
    }
}
