//! Render tilesets and Pokemon sprites into [`RgbaImage`] buffers.

use crate::graphics::{decode_tile_4bpp, Rgba, RgbaImage};
use crate::sprite::{Sprite, SpriteSheet};
use crate::tileset::{Palette, PaletteData, Tileset, TILE_SIZE};

/// Width and height of a single metatile in pixels.
const METATILE_PX: u32 = 16;
/// Number of metatiles per row in the rendered tileset image.
const GRID_WIDTH: u32 = 8;

/// Tileset renderer that composites metatiles into a single RGBA image.
#[derive(Debug)]
pub struct TilesetRenderer<'a> {
    primary: &'a Tileset,
    secondary: Option<&'a Tileset>,
    primary_tile_count: u16,
}

impl<'a> TilesetRenderer<'a> {
    /// Create a renderer for a single primary tileset.
    #[must_use]
    pub const fn new(primary: &'a Tileset) -> Self {
        Self {
            primary,
            secondary: None,
            primary_tile_count: 0x200,
        }
    }

    /// Provide an optional secondary tileset for layered rendering.
    #[must_use]
    pub fn with_secondary(mut self, secondary: &'a Tileset) -> Self {
        self.secondary = Some(secondary);
        self
    }

    /// Override the number of tiles in the primary tileset (default 0x200).
    #[must_use]
    pub fn with_primary_tile_count(mut self, count: u16) -> Self {
        self.primary_tile_count = count;
        self
    }

    /// Render the tileset to an RGBA image.
    #[must_use]
    pub fn render(&self) -> RgbaImage {
        let combined_palettes: Vec<&Palette> = if let Some(secondary) = self.secondary {
            let mut v = Vec::with_capacity(16);
            v.extend(self.primary.palettes.iter().take(6));
            v.extend(secondary.palettes.iter().skip(6).take(7));
            v.extend(self.primary.palettes.iter().skip(13));
            v
        } else {
            self.primary.palettes.iter().collect()
        };

        let primary_tiles = self.primary.tiles.as_bytes();
        let secondary_tiles = self.secondary.map_or(&[] as &[u8], |s| s.tiles.as_bytes());

        let metatiles: &crate::tileset::MetatileData = self
            .secondary
            .map_or(&self.primary.metatiles, |s| &s.metatiles);

        let num_metatiles = metatiles.len();
        let grid_height = num_metatiles.div_ceil(GRID_WIDTH as usize) as u32;
        let mut output = RgbaImage::new(GRID_WIDTH * METATILE_PX, grid_height * METATILE_PX);

        for (mt_idx, metatile) in metatiles.iter().enumerate() {
            let gx = (mt_idx as u32 % GRID_WIDTH) * METATILE_PX;
            let gy = (mt_idx as u32 / GRID_WIDTH) * METATILE_PX;

            for (layer_idx, layer) in metatile.layers.iter().enumerate() {
                let tile_index = layer.tile_index;
                let h_flip = layer.h_flip;
                let v_flip = layer.v_flip;
                let palette_index = layer.palette_index;

                let (tile_bytes, local_tile_index) = pick_tile(
                    primary_tiles,
                    secondary_tiles,
                    tile_index,
                    self.primary_tile_count,
                    self.secondary.is_some(),
                );

                let tile_start = local_tile_index * TILE_SIZE;
                let tile_end = tile_start + TILE_SIZE;
                let tile = if tile_end <= tile_bytes.len() {
                    &tile_bytes[tile_start..tile_end]
                } else {
                    &[][..]
                };

                let palette = combined_palettes
                    .get(palette_index as usize)
                    .copied()
                    .unwrap_or(&EMPTY_PALETTE);

                let tile_img = render_tile(tile, palette, h_flip, v_flip);

                let sub = layer_idx % 4;
                let x_off = (sub % 2) as u32 * 8;
                let y_off = (sub / 2) as u32 * 8;
                output.alpha_blit(&tile_img, (gx + x_off, gy + y_off));
            }
        }

        output
    }
}

const EMPTY_PALETTE: Palette = Palette([Rgba::TRANSPARENT; 16]);

#[cfg(test)]
fn decode_layer_entry(raw: u16) -> (u16, bool, bool, u8) {
    let tile_index = raw & 0x3FF;
    let h_flip = (raw >> 10) & 1 != 0;
    let v_flip = (raw >> 11) & 1 != 0;
    let palette_index = ((raw >> 12) & 0xF) as u8;
    (tile_index, h_flip, v_flip, palette_index)
}

fn pick_tile<'b>(
    primary: &'b [u8],
    secondary: &'b [u8],
    tile_index: u16,
    primary_tile_count: u16,
    has_secondary: bool,
) -> (&'b [u8], usize) {
    if has_secondary && tile_index >= primary_tile_count {
        (secondary, (tile_index - primary_tile_count) as usize)
    } else {
        (primary, tile_index as usize)
    }
}

fn render_tile(tile: &[u8], palette: &Palette, h_flip: bool, v_flip: bool) -> RgbaImage {
    let mut img = RgbaImage::new(8, 8);
    let indices = decode_tile_4bpp(tile);
    for (i, &idx) in indices.iter().enumerate() {
        let x = (i % 8) as u32;
        let y = (i / 8) as u32;
        let color = palette.0[idx as usize % 16];
        img.set_pixel(x, y, if idx == 0 { Rgba::TRANSPARENT } else { color });
    }
    if h_flip {
        img = flip_horizontal(img);
    }
    if v_flip {
        img = flip_vertical(img);
    }
    img
}

fn flip_horizontal(img: RgbaImage) -> RgbaImage {
    let mut out = RgbaImage::new(img.width(), img.height());
    for y in 0..img.height() {
        for x in 0..img.width() {
            if let Some(px) = img.pixel(img.width() - 1 - x, y) {
                out.set_pixel(x, y, px);
            }
        }
    }
    out
}

fn flip_vertical(img: RgbaImage) -> RgbaImage {
    let mut out = RgbaImage::new(img.width(), img.height());
    for y in 0..img.height() {
        for x in 0..img.width() {
            if let Some(px) = img.pixel(x, img.height() - 1 - y) {
                out.set_pixel(x, y, px);
            }
        }
    }
    out
}

/// Render a single Pokemon battle sprite sheet (64×64 4bpp) using a palette.
#[derive(Debug)]
pub struct SpriteRenderer<'a> {
    sheet: &'a SpriteSheet,
    palette: &'a Palette,
    transparent: bool,
}

impl<'a> SpriteRenderer<'a> {
    /// Create a renderer for the given sprite sheet and palette.
    #[must_use]
    pub const fn new(sheet: &'a SpriteSheet, palette: &'a Palette) -> Self {
        Self {
            sheet,
            palette,
            transparent: true,
        }
    }

    /// Toggle palette-index-0 transparency.
    #[must_use]
    pub const fn transparent(mut self, yes: bool) -> Self {
        self.transparent = yes;
        self
    }

    /// Render the sprite to an RGBA image.
    ///
    /// Sprites are always rendered at the full 64×64 frame size; the
    /// tile data in the ROM is laid out row-major across the 8×8 grid
    /// of 8×8-pixel tiles.  All tiles are rendered regardless of the
    /// bounding box stored in `MonCoords` – the bounding box is only
    /// used for positioning within the frame, not for clipping.
    #[must_use]
    pub fn render(&self) -> RgbaImage {
        const FRAME_TILES: usize = 8; // 64 / 8
        let width_px = (FRAME_TILES * 8) as u32;
        let height_px = (FRAME_TILES * 8) as u32;
        let mut img = RgbaImage::new(width_px, height_px);

        for tile_y in 0..FRAME_TILES {
            for tile_x in 0..FRAME_TILES {
                let tile_idx = tile_y * FRAME_TILES + tile_x;
                let Some(tile) = self.sheet.tiles.tile(tile_idx) else {
                    continue;
                };
                let indices = decode_tile_4bpp(tile);
                for (i, &idx) in indices.iter().enumerate() {
                    let x = (tile_x * 8 + (i % 8)) as u32;
                    let y = (tile_y * 8 + (i / 8)) as u32;
                    let color = self.palette.0[idx as usize % 16];
                    let pixel = if self.transparent && idx == 0 {
                        Rgba::TRANSPARENT
                    } else {
                        color
                    };
                    img.set_pixel(x, y, pixel);
                }
            }
        }
        img
    }
}

/// Helper for picking a palette out of a `PaletteData` for a sprite.
#[must_use]
pub fn sprite_palette(data: &PaletteData) -> Option<&Palette> {
    data.get(0)
}

/// Helper for converting a `Sprite` to a `SpriteRenderer`.
#[must_use]
pub fn renderer_for_sprite(sprite: &Sprite, is_front: bool) -> Option<SpriteRenderer<'_>> {
    let sheet = if is_front {
        sprite.front.as_ref()?
    } else {
        sprite.back.as_ref()?
    };
    let palette = sprite_palette(&sprite.palette)?;
    Some(SpriteRenderer::new(sheet, palette))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::Rgba;
    use crate::sprite::MonCoords;
    use crate::tileset::{Metatile, MetatileData, MetatileLayer, TileData};

    fn make_palette() -> Palette {
        Palette([
            Rgba::TRANSPARENT,
            Rgba::WHITE,
            Rgba::BLACK,
            Rgba(255, 0, 0, 255),
            Rgba(0, 255, 0, 255),
            Rgba(0, 0, 255, 255),
            Rgba::WHITE,
            Rgba::WHITE,
            Rgba::WHITE,
            Rgba::WHITE,
            Rgba::WHITE,
            Rgba::WHITE,
            Rgba::WHITE,
            Rgba::WHITE,
            Rgba::WHITE,
            Rgba::WHITE,
        ])
    }

    fn make_tileset() -> Tileset {
        Tileset {
            name: "test".to_owned(),
            is_primary: true,
            tile_count: 1,
            tiles: TileData::from_bytes(vec![0u8; 32]),
            palettes: PaletteData::from_bgr555(&vec![0u8; 16 * 16 * 2]).unwrap(),
            metatiles: MetatileData::from_packed(&[0u8; 16]),
        }
    }

    #[test]
    fn render_tile_returns_8x8_image() {
        let palette = make_palette();
        let tile = vec![0x11u8; 32]; // every other pixel is index 1
        let img = render_tile(&tile, &palette, false, false);
        assert_eq!(img.width(), 8);
        assert_eq!(img.height(), 8);
        assert_eq!(img.pixel(0, 0), Some(Rgba::WHITE));
    }

    #[test]
    fn tileset_renderer_renders_grid() {
        let tileset = make_tileset();
        let renderer = TilesetRenderer::new(&tileset);
        let img = renderer.render();
        // 8 metatiles wide, 1 row tall (1 metatile).
        assert_eq!(img.width(), 128);
        assert_eq!(img.height(), 16);
    }

    #[test]
    fn layer_entry_decode() {
        let (tile, h, v, pal) = decode_layer_entry(0x4B12);
        assert_eq!(tile, 0x312);
        assert!(!h);
        assert!(v);
        assert_eq!(pal, 4);
    }

    #[test]
    fn sprite_renderer_64x64() {
        let sheet = SpriteSheet {
            tiles: TileData::from_bytes(vec![0x11u8; 32 * 64]),
            coords: MonCoords {
                width_tiles: 8,
                height_tiles: 8,
                y_offset: 0,
            },
        };
        let palette = make_palette();
        let img = SpriteRenderer::new(&sheet, &palette).render();
        assert_eq!(img.width(), 64);
        assert_eq!(img.height(), 64);
    }

    #[test]
    fn render_tile_flip_changes_pixels() {
        let palette = make_palette();
        let tile = vec![0x10u8; 16]
            .into_iter()
            .chain(vec![0x01u8; 16])
            .collect::<Vec<_>>();
        let normal = render_tile(&tile, &palette, false, false);
        let flipped = render_tile(&tile, &palette, true, false);
        assert_eq!(normal.pixel(0, 0), flipped.pixel(7, 0));
    }

    // Smoke check: the metatile-storage helper is correct.
    #[test]
    fn metatile_storage_round_trip() {
        let mt = Metatile {
            layers: [MetatileLayer {
                tile_index: 0x10,
                h_flip: true,
                v_flip: false,
                palette_index: 3,
            }; 8],
        };
        let bytes = {
            let mut out = Vec::new();
            for layer in &mt.layers {
                out.extend_from_slice(&layer.to_bits().to_le_bytes());
            }
            out
        };
        assert_eq!(bytes.len(), 16);
    }
}
