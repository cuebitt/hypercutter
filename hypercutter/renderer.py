import logging
from typing import Any

from PIL import Image

from .lzss3 import decompress_bytes
from .utils import decode_bgr555, decode_tile_4bpp, parse_metatile_entry

logger = logging.getLogger(__name__)


class TilesetRenderer:
    def __init__(self, tileset_data: dict[str, Any], rom_data: bytes = None):
        """
        Initialize the renderer with tileset data.

        Args:
            tileset_data: Extracted tileset data containing primary and secondary tilesets.
            rom_data: Optional ROM bytes for re-extracting raw data if not present.
        """
        self.rom = rom_data
        self.primary = tileset_data.get("primary")
        self.secondary = tileset_data.get("secondary")
        self._ensure_raw_data()

    def _extract_raw_from_rom(self, tileset: dict[str, Any]) -> None:
        """Extract raw data from ROM using pointers if not present."""
        if self.rom is None:
            return
        if tileset.get("palettes_raw") is None:
            ptr = tileset.get("palettes_ptr")
            if ptr:
                offset = ptr - 0x8000000
                if offset > 0 and offset + 512 <= len(self.rom):
                    tileset["palettes_raw"] = bytes(self.rom[offset : offset + 512])
        if tileset.get("tiles_raw") is None:
            ptr = tileset.get("tiles_ptr")
            length = tileset.get("tiles_length", 0)
            is_compressed = tileset.get("is_compressed", False)
            if ptr and length:
                offset = ptr - 0x8000000
                if offset > 0 and offset + length <= len(self.rom):
                    raw_data = bytes(self.rom[offset : offset + length])
                    if is_compressed:
                        try:
                            tileset["tiles_raw"] = bytes(decompress_bytes(raw_data))
                        except Exception:
                            tileset["tiles_raw"] = raw_data
                    else:
                        tileset["tiles_raw"] = raw_data
        if tileset.get("metatiles_raw") is None:
            ptr = tileset.get("metatiles_ptr")
            length = tileset.get("metatiles_length", 0)
            if ptr and length:
                offset = ptr - 0x8000000
                if offset > 0 and offset + length <= len(self.rom):
                    tileset["metatiles_raw"] = bytes(self.rom[offset : offset + length])

    def _ensure_raw_data(self) -> None:
        """Ensure raw data is present, re-extracting from ROM if needed."""
        for tileset in (self.primary, self.secondary):
            if tileset:
                for field in ("palettes_raw", "tiles_raw", "metatiles_raw"):
                    if not tileset.get(field):
                        self._extract_raw_from_rom(tileset)

    def _get_palettes(
        self, tileset: dict[str, Any]
    ) -> list[list[tuple[int, int, int]]]:
        """Decode all 16 palettes for a tileset."""
        raw_palettes = tileset.get("palettes_raw", b"")
        palettes = []
        for i in range(16):
            palette = []
            for j in range(16):
                offset = (i * 16 + j) * 2
                if offset + 2 <= len(raw_palettes):
                    color_val = int.from_bytes(
                        raw_palettes[offset : offset + 2], "little"
                    )
                    palette.append(decode_bgr555(color_val))
                else:
                    palette.append((0, 0, 0))
            palettes.append(palette)
        return palettes

    def _render_tile(
        self,
        tile_data: bytes,
        palette: list[tuple[int, int, int]],
        h_flip: bool = False,
        v_flip: bool = False,
        is_transparent: bool = False,
    ) -> Image.Image:
        """Render a single 8x8 tile."""
        indices = decode_tile_4bpp(tile_data)
        img = Image.new("RGBA", (8, 8))
        pixels = img.load()

        for y in range(8):
            for x in range(8):
                idx = indices[y * 8 + x]
                if is_transparent and idx == 0:
                    pixels[x, y] = (0, 0, 0, 0)
                else:
                    r, g, b = palette[idx]
                    pixels[x, y] = (r, g, b, 255)

        if h_flip:
            img = img.transpose(Image.FLIP_LEFT_RIGHT)
        if v_flip:
            img = img.transpose(Image.FLIP_TOP_BOTTOM)

        return img

    def render(self) -> Image.Image:
        """
        Render the tileset as a 16x16 metatile grid.

        Returns:
            A PIL Image of the rendered tileset.
        """
        logger.debug("Starting render")
        if not self.primary:
            raise ValueError("Primary tileset data is required for rendering")

        p_palettes = self._get_palettes(self.primary)
        s_palettes = self._get_palettes(self.secondary) if self.secondary else []

        if self.secondary:
            combined_palettes = p_palettes[:6] + s_palettes[6:13] + p_palettes[13:]
        else:
            combined_palettes = p_palettes

        # Combine tiles: primary tiles are usually first, followed by secondary
        p_tiles = self.primary.get("tiles_raw", b"")
        s_tiles = self.secondary.get("tiles_raw", b"") if self.secondary else b""

        # Metatiles are 16-bit indices (8 per metatile)
        mt_data = (
            self.secondary.get("metatiles_raw", b"")
            if self.secondary
            else self.primary.get("metatiles_raw", b"")
        )
        num_metatiles = len(mt_data) // 16

        # Standard grid width: 8 metatiles
        grid_width = 8
        grid_height = (num_metatiles + grid_width - 1) // grid_width

        output = Image.new("RGBA", (grid_width * 16, grid_height * 16))

        for mt_idx in range(num_metatiles):
            mt_offset = mt_idx * 16
            metatile_img = Image.new("RGBA", (16, 16))

            # Each metatile has 8 tiles: 4 bottom layer, 4 top layer
            for i in range(8):
                entry_offset = mt_offset + i * 2
                entry = int.from_bytes(
                    mt_data[entry_offset : entry_offset + 2], "little"
                )
                tile_idx, h_flip, v_flip, pal_idx = parse_metatile_entry(entry)

                # Determine which tileset to pull from
                # Primary tileset has 512 tiles (0x200)
                # If tile_idx < 0x200, it's primary. Else, it's secondary (relative to 0x200)
                is_secondary_tile = tile_idx >= 0x200
                if self.secondary:
                    current_tiles = s_tiles if is_secondary_tile else p_tiles
                    local_tile_idx = tile_idx - 0x200 if is_secondary_tile else tile_idx
                elif self.primary.get("is_secondary"):
                    current_tiles = p_tiles
                    local_tile_idx = tile_idx - 0x200 if is_secondary_tile else tile_idx
                else:
                    current_tiles = p_tiles
                    local_tile_idx = tile_idx

                tile_bytes_offset = local_tile_idx * 32
                if tile_bytes_offset + 32 > len(current_tiles):
                    tile_bytes = b"\x00" * 32
                else:
                    tile_bytes = current_tiles[
                        tile_bytes_offset : tile_bytes_offset + 32
                    ]

                # Palette index 0 is always transparent for both layers in many GBA games,
                # but especially for the top layer to show the bottom layer.
                # Usually, the first color of every palette is transparent.
                # is_top_layer = i >= 4
                tile_img = self._render_tile(
                    tile_bytes,
                    combined_palettes[pal_idx],
                    h_flip,
                    v_flip,
                    is_transparent=True,  # Both layers respect transparency in Metatiles
                )

                # Paste into the 16x16 metatile
                # 0: bottom-left, 1: bottom-right, 2: top-left, 3: top-right (Wait, GBA order is usually different)
                # In Emerald:
                # 0-3 are Layer 1 (Bottom), 4-7 are Layer 2 (Top)
                # Each layer: [Top-Left, Top-Right, Bottom-Left, Bottom-Right]
                sub_idx = i % 4
                x_off = (sub_idx % 2) * 8
                y_off = (sub_idx // 2) * 8

                metatile_img.alpha_composite(tile_img, (x_off, y_off))

            # Paste metatile into grid
            gx = (mt_idx % grid_width) * 16
            gy = (mt_idx // grid_width) * 16
            output.paste(metatile_img, (gx, gy))

        logger.debug("Render complete: %d metatiles", num_metatiles)
        return output
