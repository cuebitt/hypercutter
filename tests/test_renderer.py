import pytest

from hypercutter.renderer import TilesetRenderer


class TestTilesetRenderer:
    def test_requires_primary_tileset(self):
        with pytest.raises(ValueError, match="Primary tileset data is required"):
            TilesetRenderer({}).render()

    def test_stores_rom_base_address(self):
        data = {"primary": {"tiles_raw": b"", "palettes_raw": b"", "metatiles_raw": b""}}
        renderer = TilesetRenderer(data, rom_base_address=0x9000000)
        assert renderer.rom_base_address == 0x9000000

    def test_uses_default_rom_base_address(self):
        data = {"primary": {"tiles_raw": b"", "palettes_raw": b"", "metatiles_raw": b""}}
        renderer = TilesetRenderer(data)
        assert renderer.rom_base_address == 0x8000000

    def test_extracts_from_rom_when_missing(self):
        # Create ROM with realistic pointer values
        # Real ROMs have pointers like 0x8xxxxxx
        rom = bytearray(0x300000)
        # Place data at offset 0x100000 (address 0x8100000)
        for i in range(16):
            rom[0x100000 + i * 2] = i * 4  # R
            rom[0x100001 + i * 2] = i * 4  # G

        tileset_data = {
            "primary": {
                "palettes_ptr": 0x8100000,
                "tiles_ptr": 0x8200000,
                "tiles_length": 32,
                "tiles_raw": None,
                "palettes_raw": None,
                "metatiles_raw": None,
            }
        }

        renderer = TilesetRenderer(tileset_data, bytes(rom), rom_base_address=0x8000000)
        assert renderer.primary["palettes_raw"] is not None
        assert renderer.primary["tiles_raw"] is not None


class TestRenderTile:
    def test_renders_8x8_image(self):
        data = {"primary": {"tiles_raw": b"\x00" * 32, "palettes_raw": b"\x00" * 512}}
        renderer = TilesetRenderer(data)

        # Create a simple 4bpp tile (all index 1)
        tile_data = bytes([0x11] * 32)
        palette = [(0, 0, 0), (255, 0, 0), (0, 255, 0), (0, 0, 255)] * 4

        img = renderer._render_tile(tile_data, palette)
        assert img.size == (8, 8)
        assert img.mode == "RGBA"

    def test_respects_transparency(self):
        data = {"primary": {"tiles_raw": b"", "palettes_raw": b""}}
        renderer = TilesetRenderer(data)

        tile_data = bytes([0x00] * 32)  # All index 0
        palette = [(255, 0, 0), (0, 255, 0)]  # Index 0 = red

        img = renderer._render_tile(tile_data, palette, is_transparent=True)
        # First pixel should be transparent
        assert img.getpixel((0, 0))[3] == 0

    def test_applies_horizontal_flip(self):
        data = {"primary": {"tiles_raw": b"", "palettes_raw": b""}}
        renderer = TilesetRenderer(data)

        # Create a tile with left half different from right half
        tile_data = bytes([0x10] * 16 + [0x01] * 16)
        palette = [(0, 0, 0), (255, 255, 255)]

        img_normal = renderer._render_tile(tile_data, palette, h_flip=False)
        img_flipped = renderer._render_tile(tile_data, palette, h_flip=True)

        # First column of normal should match last column of flipped
        assert img_normal.getpixel((0, 0)) == img_flipped.getpixel((7, 0))

    def test_applies_vertical_flip(self):
        data = {"primary": {"tiles_raw": b"", "palettes_raw": b""}}
        renderer = TilesetRenderer(data)

        tile_data = bytes([0x10] * 16 + [0x01] * 16)
        palette = [(0, 0, 0), (255, 255, 255)]

        img_normal = renderer._render_tile(tile_data, palette, v_flip=False)
        img_flipped = renderer._render_tile(tile_data, palette, v_flip=True)

        # First row of normal should match last row of flipped
        assert img_normal.getpixel((0, 0)) == img_flipped.getpixel((0, 7))
