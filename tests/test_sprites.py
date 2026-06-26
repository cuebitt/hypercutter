"""Tests for Pokemon sprite extraction functionality."""

import struct
import pytest
from hypercutter.classes import MonCoords, SpritePalette, SpriteSheet
from hypercutter.extractors import (
    extract_mon_coords,
    extract_sprite_palette,
    extract_sprite_sheet,
)
from hypercutter.sprite_renderer import PokemonSpriteRenderer, get_species_name


class TestSpriteSheet:
    def test_extracts_correct_values(self):
        data = struct.pack("<IHH", 0x08100000, 0x1000, 0x0001)
        result = extract_sprite_sheet(data, 0, 0x08000000)
        assert isinstance(result, SpriteSheet)
        assert result.data_ptr == 0x08100000
        assert result.size == 0x1000
        assert result.tag == 0x0001

    def test_raises_on_invalid_offset(self):
        data = bytes(0x10)
        with pytest.raises(ValueError, match="out of range"):
            extract_sprite_sheet(data, 0x100, 0x08000000)


class TestSpritePalette:
    def test_extracts_correct_values(self):
        data = struct.pack("<IHH", 0x08200000, 0x0001, 0x0000)
        result = extract_sprite_palette(data, 0, 0x08000000)
        assert isinstance(result, SpritePalette)
        assert result.data_ptr == 0x08200000
        assert result.tag == 0x0001

    def test_raises_on_invalid_offset(self):
        data = bytes(0x10)
        with pytest.raises(ValueError, match="out of range"):
            extract_sprite_palette(data, 0x100, 0x08000000)


class TestMonCoords:
    def test_extracts_correct_values(self):
        # size = 0x45 means width=4 tiles (32px), height=5 tiles (40px)
        # 4 bytes: size, y_offset, padding, padding
        data = struct.pack("<BBBB", 0x45, 14, 0x00, 0x00)
        result = extract_mon_coords(data, 0)
        assert isinstance(result, MonCoords)
        assert result.size == 0x45
        assert result.y_offset == 14

    def test_width_pixels(self):
        data = struct.pack("<BBBB", 0x45, 0, 0x00, 0x00)  # width=4 tiles
        result = extract_mon_coords(data, 0)
        assert result.width_pixels == 32  # 4 * 8

    def test_height_pixels(self):
        data = struct.pack("<BBBB", 0x45, 0, 0x00, 0x00)  # height=5 tiles
        result = extract_mon_coords(data, 0)
        assert result.height_pixels == 40  # 5 * 8

    def test_raises_on_invalid_offset(self):
        data = bytes(0x10)
        with pytest.raises(ValueError, match="out of range"):
            extract_mon_coords(data, 0x100)


class TestSpriteRenderer:
    def test_renders_64x64_image(self):
        # Full 64x64 frame = 64 tiles * 32 bytes = 2048 bytes
        tile_data = bytes([0x11] * 32) + b"\x00" * (2048 - 32)
        palette_data = struct.pack("<HH", 0x0000, 0x001F) + b"\x00" * 28

        renderer = PokemonSpriteRenderer(tile_data, palette_data)
        img = renderer.render()
        assert img.size == (64, 64)
        assert img.mode == "RGBA"

    def test_respects_transparency(self):
        tile_data = b"\x00" * 2048
        palette_data = struct.pack("<HH", 0x7C00, 0x001F) + b"\x00" * 28

        renderer = PokemonSpriteRenderer(tile_data, palette_data)
        img = renderer.render(is_transparent=True)
        pixel = img.getpixel((0, 0))
        assert pixel[3] == 0  # Transparent

    def test_decode_palette(self):
        palette_data = struct.pack("<HH", 0x7FFF, 0x001F) + b"\x00" * 28

        renderer = PokemonSpriteRenderer(b"", palette_data)
        palette = renderer.decode_palette()
        assert palette[0] == (255, 255, 255)  # White
        assert palette[1] == (255, 0, 0)  # Red


class TestGetSpeciesName:
    def test_known_species(self):
        assert get_species_name(0) == "missing"
        assert get_species_name(1) == "bulbasaur"
        assert get_species_name(25) == "pikachu"
        assert get_species_name(410) == "deoxys"

    def test_unknown_species(self):
        assert get_species_name(999) == "unknown_999"
