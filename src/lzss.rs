//! LZSS10 / LZSS11 decompression.
//!
//! Thin wrapper over the [`nintendo_lz`] crate that normalizes the
//! [`nintendo_lz::decompress_arr`] error into our [`Error::Decompression`]
//! variant.

use crate::error::{Error, Result};

/// Decompress an LZSS-compressed buffer.
///
/// Inspects the 4-byte LZSS header (magic + 24-bit uncompressed size) and
/// dispatches to the LZ10 or LZ11 decompressor.
///
/// # Errors
///
/// Returns [`Error::InvalidLzssMagic`] if the header byte is not `0x10` or
/// `0x11` or if the input is empty. Returns [`Error::Decompression`] if the
/// underlying decompressor rejects the data.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    let Some(&first) = data.first() else {
        return Err(Error::InvalidLzssMagic { magic: 0 });
    };
    if first != 0x10 && first != 0x11 {
        return Err(Error::InvalidLzssMagic { magic: first });
    }
    nintendo_lz::decompress_arr(data).map_err(|source| Error::Decompression { message: source.to_string() })
}

/// Returns `true` if the buffer starts with a valid LZSS magic byte
/// (`0x10` or `0x11`).
#[must_use]
pub const fn is_lzss(data: &[u8]) -> bool {
    matches!(data.first(), Some(0x10) | Some(0x11))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_invalid_magic() {
        let err = decompress(&[]).unwrap_err();
        assert!(matches!(err, Error::InvalidLzssMagic { magic: 0 }));
    }

    #[test]
    fn bad_magic_returns_error() {
        let err = decompress(&[0xFF, 0, 0, 0]).unwrap_err();
        assert!(matches!(err, Error::InvalidLzssMagic { magic: 0xFF }));
    }

    #[test]
    fn is_lzss_recognizes_magic_bytes() {
        assert!(is_lzss(&[0x10]));
        assert!(is_lzss(&[0x11]));
        assert!(!is_lzss(&[0x12]));
        assert!(!is_lzss(&[]));
    }

    #[test]
    fn decompress_lz10_roundtrip() {
        // Build an LZ10 stream containing a single literal byte.
        // Header: 0x10 | (size << 8). For size=1, header = 0x10 | 0x100 = 0x00000110.
        let mut data = vec![0x10, 0x01, 0x00, 0x00];
        // Flag byte 0x00 → all 8 slots are literal copies.
        data.push(0x00);
        // 1 literal byte.
        data.push(0xAA);
        // Trailing FF marks end of stream in nintendo_lz's format? Not strictly needed.
        let out = decompress(&data).unwrap();
        assert_eq!(out, vec![0xAA]);
    }
}
