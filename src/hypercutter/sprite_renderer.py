"""
Sprite renderer for Pokemon battle sprites.

Handles decoding 4bpp tile data and BGR555 palettes to render
Pokemon sprites as PIL Images with transparency support.
"""

import logging

from PIL import Image

from .constants import MON_PIC_HEIGHT, MON_PIC_WIDTH, TILE_SIZE
from .graphics import decode_palettes, render_pixels_to_image
from .types import SpriteEntry
from .utils import decode_tile_4bpp

__all__ = ["PokemonSpriteRenderer", "get_species_name"]

logger = logging.getLogger(__name__)


def get_species_name(species_id: int, species_names: list[str] | None = None) -> str:
    """Get the lowercase species name for a given ID."""
    if species_names and 0 <= species_id < len(species_names):
        return species_names[species_id]
    return f"unknown_{species_id:03d}"


class PokemonSpriteRenderer:
    """Render Pokemon sprites from extracted data."""

    def __init__(
        self,
        tile_data: bytes,
        palette_data: bytes,
    ):
        """
        Initialize the renderer with sprite data.

        Args:
            tile_data: Decompressed 4bpp tile data (full 64x64 frame).
            palette_data: Decompressed BGR555 palette data (16 colors).
        """
        self.tile_data = tile_data
        self.palette_data = palette_data

    def decode_palette(self) -> list[tuple[int, int, int]]:
        """
        Decode BGR555 palette data to RGB tuples.

        Returns:
            List of (r, g, b) tuples, one per color.
        """
        palettes = decode_palettes(self.palette_data, num_palettes=1)
        return palettes[0] if palettes else [(0, 0, 0)] * 16

    def decode_tiles(self) -> list[list[int]]:
        """
        Decode 4bpp tile data into a 2D grid of pixel indices.

        The sprite data is always a full 64x64 frame (8x8 tiles).
        The actual sprite is positioned within this frame via y_offset.

        Returns:
            2D list of pixel indices [y][x] for the full 64x64 frame.
        """
        frame_width = MON_PIC_WIDTH
        frame_height = MON_PIC_HEIGHT
        tiles_x = MON_PIC_WIDTH // 8
        tiles_y = MON_PIC_HEIGHT // 8

        pixels: list[list[int]] = [[0] * frame_width for _ in range(frame_height)]

        tile_size = TILE_SIZE

        for tile_y in range(tiles_y):
            for tile_x in range(tiles_x):
                tile_idx = tile_y * tiles_x + tile_x
                tile_offset = tile_idx * tile_size

                if tile_offset + tile_size > len(self.tile_data):
                    logger.debug(
                        "Tile %d out of bounds: offset=%d size=%d data_len=%d",
                        tile_idx,
                        tile_offset,
                        tile_size,
                        len(self.tile_data),
                    )
                    continue

                tile_data = self.tile_data[tile_offset : tile_offset + tile_size]
                tile_pixels = decode_tile_4bpp(tile_data)

                for py in range(8):
                    for px in range(8):
                        src_idx = py * 8 + px
                        dst_x = tile_x * 8 + px
                        dst_y = tile_y * 8 + py

                        if dst_x < frame_width and dst_y < frame_height:
                            pixels[dst_y][dst_x] = tile_pixels[src_idx]

        return pixels

    def render(self, is_transparent: bool = True) -> Image.Image:
        """
        Render the sprite as a 64x64 RGBA PIL Image.

        Args:
            is_transparent: If True, palette index 0 is treated as transparent.

        Returns:
            64x64 RGBA PIL Image of the sprite frame.
        """
        palette = self.decode_palette()
        pixel_indices = self.decode_tiles()

        return render_pixels_to_image(
            pixel_indices, palette, MON_PIC_WIDTH, MON_PIC_HEIGHT, is_transparent
        )

    @staticmethod
    def render_spritesheet(
        sprites: list[Image.Image],
        columns: int = 8,
        padding: int = 1,
        background: tuple[int, int, int, int] = (0, 0, 0, 0),
    ) -> Image.Image:
        """
        Combine multiple sprite images into a spritesheet.

        Args:
            sprites: List of PIL Images to combine.
            columns: Number of columns in the spritesheet.
            padding: Pixels of padding between sprites.
            background: RGBA background color.

        Returns:
            Combined spritesheet as a PIL Image.
        """
        if not sprites:
            return Image.new("RGBA", (1, 1), background)

        # Find maximum dimensions
        max_width = max(s.width for s in sprites)
        max_height = max(s.height for s in sprites)

        # Calculate grid dimensions
        rows = (len(sprites) + columns - 1) // columns
        sheet_width = columns * (max_width + padding) - padding
        sheet_height = rows * (max_height + padding) - padding

        # Create spritesheet
        sheet = Image.new("RGBA", (sheet_width, sheet_height), background)

        for i, sprite in enumerate(sprites):
            col = i % columns
            row = i // columns
            x = col * (max_width + padding)
            y = row * (max_height + padding)
            sheet.paste(sprite, (x, y), sprite)

        return sheet

    @classmethod
    def from_sprite_data(
        cls,
        sprite_data: SpriteEntry,
        is_front: bool = True,
        is_shiny: bool = False,
    ) -> "PokemonSpriteRenderer":
        """
        Create a renderer from extracted sprite data.

        Args:
            sprite_data: Dictionary from extract_all_pokemon_sprites.
            is_front: If True, use front tile data; otherwise back.
            is_shiny: If True, use shiny palette.

        Returns:
            PokemonSpriteRenderer instance.
        """
        tile_key = "front_tile_data" if is_front else "back_tile_data"
        pal_key = "shiny_palette_data" if is_shiny else "palette_data"

        return cls(
            tile_data=sprite_data[tile_key],
            palette_data=sprite_data[pal_key],
        )

    @classmethod
    def from_back_sprite_data(
        cls,
        sprite_data: SpriteEntry,
        is_shiny: bool = False,
    ) -> "PokemonSpriteRenderer":
        """Create a renderer from extracted back sprite data."""
        return cls.from_sprite_data(sprite_data, is_front=False, is_shiny=is_shiny)
