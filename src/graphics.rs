//! GBA graphics primitives: BGR555 decoding, 4bpp tile decoding, RGBA buffers.

use std::io::Write;
use std::path::Path;

use crate::error::{Error, Result};

/// An RGBA color with 8 bits per channel.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self(0, 0, 0, 0);
    /// Fully opaque black.
    pub const BLACK: Self = Self(0, 0, 0, 255);
    /// Fully opaque white.
    pub const WHITE: Self = Self(255, 255, 255, 255);
}

/// Convert a 16-bit BGR555 color to 8-bit RGBA with full opacity.
///
/// The GBA stores 5 bits per channel in the order B-G-R, with the top bit
/// unused. Each 5-bit value is scaled to 8 bits by multiplying by 8.
#[must_use]
pub const fn bgr555_to_rgba(c: u16) -> Rgba {
    let r5 = c & 0x1F;
    let g5 = (c >> 5) & 0x1F;
    let b5 = (c >> 10) & 0x1F;
    Rgba(scale_5_to_8(r5), scale_5_to_8(g5), scale_5_to_8(b5), 255)
}

const fn scale_5_to_8(v: u16) -> u8 {
    (v * 8) as u8
}

/// Decode 32 bytes of 4bpp tile data into 64 palette indices (row-major, 8×8).
///
/// If the input is shorter than 32 bytes, the missing bytes are treated as
/// zero (palette index 0, transparent).
#[must_use]
pub fn decode_tile_4bpp(data: &[u8]) -> [u8; 64] {
    let mut out = [0u8; 64];
    for (i, byte) in data.iter().take(32).enumerate() {
        out[i * 2] = byte & 0x0F;
        out[i * 2 + 1] = byte >> 4;
    }
    out
}

/// A row-major RGBA image buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RgbaImage {
    /// Create a new image filled with [`Rgba::TRANSPARENT`].
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    /// Construct an image from a raw RGBA byte buffer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the buffer length does not match
    /// `width * height * 4`.
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self> {
        let expected = width as usize * height as usize * 4;
        if pixels.len() != expected {
            return Err(Error::Io {
                path: std::path::PathBuf::from("<rgba buffer>"),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "pixel buffer length {} does not match width*height*4 = {expected}",
                        pixels.len()
                    ),
                ),
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// Returns the image width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the image height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the raw RGBA byte buffer (length = `width * height * 4`).
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.pixels
    }

    /// Read a single pixel.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<Rgba> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        Some(Rgba(
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        ))
    }

    /// Set a single pixel.
    pub fn set_pixel(&mut self, x: u32, y: u32, px: Rgba) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        self.pixels[idx] = px.0;
        self.pixels[idx + 1] = px.1;
        self.pixels[idx + 2] = px.2;
        self.pixels[idx + 3] = px.3;
    }

    /// Copy `src` onto `self` at `(dx, dy)` with no alpha blending (src fully replaces dst).
    pub fn blit(&mut self, src: &RgbaImage, dst: (u32, u32)) {
        blit_inner(self, src, dst, false);
    }

    /// Alpha-blend `src` onto `self` at `(dx, dy)`.
    pub fn alpha_blit(&mut self, src: &RgbaImage, dst: (u32, u32)) {
        blit_inner(self, src, dst, true);
    }

    /// Encode the image as PNG and write it to `writer`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Png`] if the PNG encoder fails, or [`Error::Io`] if
    /// writing to the underlying stream fails.
    pub fn write_png(&self, mut writer: impl Write) -> Result<()> {
        let mut encoder = png::Encoder::new(&mut writer, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&self.pixels)?;
        Ok(())
    }

    /// Encode the image as PNG and save it to a file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the file cannot be opened or written, or
    /// [`Error::Png`] if encoding fails.
    pub fn save_png(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let file = std::fs::File::create(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        self.write_png(file)
    }
}

fn blit_inner(dst: &mut RgbaImage, src: &RgbaImage, (dx, dy): (u32, u32), alpha: bool) {
    let sw = src.width;
    let sh = src.height;
    for sy in 0..sh {
        let dy = dy + sy;
        if dy >= dst.height {
            break;
        }
        for sx in 0..sw {
            let dx = dx + sx;
            if dx >= dst.width {
                break;
            }
            let s_idx = ((sy * sw + sx) * 4) as usize;
            let d_idx = ((dy * dst.width + dx) * 4) as usize;
            if alpha {
                let sa = u32::from(src.pixels[s_idx + 3]);
                if sa == 0 {
                    continue;
                }
                if sa == 255 {
                    dst.pixels[d_idx..d_idx + 4].copy_from_slice(&src.pixels[s_idx..s_idx + 4]);
                } else {
                    let da = u32::from(dst.pixels[d_idx + 3]);
                    let inv = 255 - sa;
                    let sr = u32::from(src.pixels[s_idx]);
                    let sg = u32::from(src.pixels[s_idx + 1]);
                    let sb = u32::from(src.pixels[s_idx + 2]);
                    let dr = u32::from(dst.pixels[d_idx]);
                    let dg = u32::from(dst.pixels[d_idx + 1]);
                    let db = u32::from(dst.pixels[d_idx + 2]);
                    let out_a = sa + da * inv / 255;
                    let denom = out_a.max(1);
                    dst.pixels[d_idx] = ((sr * sa + dr * da * inv / 255) / denom) as u8;
                    dst.pixels[d_idx + 1] = ((sg * sa + dg * da * inv / 255) / denom) as u8;
                    dst.pixels[d_idx + 2] = ((sb * sa + db * da * inv / 255) / denom) as u8;
                    dst.pixels[d_idx + 3] = out_a as u8;
                }
            } else {
                dst.pixels[d_idx..d_idx + 4].copy_from_slice(&src.pixels[s_idx..s_idx + 4]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgr555_black_is_rgba_black() {
        assert_eq!(bgr555_to_rgba(0x0000), Rgba::BLACK);
    }

    #[test]
    fn bgr555_white_is_rgba_white() {
        assert_eq!(bgr555_to_rgba(0x7FFF), Rgba(248, 248, 248, 255));
    }

    #[test]
    fn bgr555_pure_red() {
        // Low bit is R; pure red is R=31, G=0, B=0 → 0x001F
        assert_eq!(bgr555_to_rgba(0x001F), Rgba(248, 0, 0, 255));
    }

    #[test]
    fn bgr555_pure_green() {
        // Middle bits are G; pure green is G=31, R=0, B=0 → 0x03E0
        assert_eq!(bgr555_to_rgba(0x03E0), Rgba(0, 248, 0, 255));
    }

    #[test]
    fn bgr555_pure_blue() {
        // High bits are B; pure blue is B=31, R=0, G=0 → 0x7C00
        assert_eq!(bgr555_to_rgba(0x7C00), Rgba(0, 0, 248, 255));
    }

    #[test]
    fn decode_tile_4bpp_byte_order() {
        // 0x12 → low nibble 2, high nibble 1
        let data = [0x12u8; 32];
        let decoded = decode_tile_4bpp(&data);
        assert_eq!(decoded[0], 2);
        assert_eq!(decoded[1], 1);
        assert_eq!(decoded[2], 2);
        assert_eq!(decoded[3], 1);
        assert_eq!(decoded.len(), 64);
    }

    #[test]
    fn rgba_image_new_is_zero_size() {
        let img = RgbaImage::new(2, 2);
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
        assert_eq!(img.as_bytes().len(), 16);
        assert_eq!(img.pixel(0, 0), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn rgba_image_set_and_get_pixel() {
        let mut img = RgbaImage::new(4, 4);
        img.set_pixel(2, 3, Rgba(10, 20, 30, 40));
        assert_eq!(img.pixel(2, 3), Some(Rgba(10, 20, 30, 40)));
        assert_eq!(img.pixel(0, 0), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn rgba_image_pixel_out_of_bounds() {
        let img = RgbaImage::new(2, 2);
        assert_eq!(img.pixel(2, 0), None);
        assert_eq!(img.pixel(0, 2), None);
    }

    #[test]
    fn rgba_image_blit_replaces() {
        let mut dst = RgbaImage::new(4, 4);
        let mut src = RgbaImage::new(2, 2);
        src.set_pixel(0, 0, Rgba::WHITE);
        src.set_pixel(1, 1, Rgba::BLACK);
        dst.blit(&src, (1, 1));
        assert_eq!(dst.pixel(1, 1), Some(Rgba::WHITE));
        assert_eq!(dst.pixel(2, 2), Some(Rgba::BLACK));
        assert_eq!(dst.pixel(0, 0), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn rgba_image_alpha_blit_skips_zero_alpha() {
        let mut dst = RgbaImage::new(2, 2);
        dst.set_pixel(0, 0, Rgba::WHITE);
        let mut src = RgbaImage::new(1, 1);
        src.set_pixel(0, 0, Rgba::TRANSPARENT);
        dst.alpha_blit(&src, (0, 0));
        assert_eq!(dst.pixel(0, 0), Some(Rgba::WHITE));
    }

    #[test]
    fn rgba_image_alpha_blit_blends_opaque() {
        let mut dst = RgbaImage::new(2, 2);
        dst.set_pixel(0, 0, Rgba(50, 60, 70, 200));
        let mut src = RgbaImage::new(1, 1);
        src.set_pixel(0, 0, Rgba(200, 100, 50, 100));
        dst.alpha_blit(&src, (0, 0));
        // Verifying the output landed at (0, 0) and isn't the original.
        let px = dst.pixel(0, 0).unwrap();
        assert_ne!(px, Rgba(50, 60, 70, 200));
    }
}
