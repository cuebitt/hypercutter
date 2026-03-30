import struct
from hypercutter.classes import MapLayout, Offset, OffsetType, Tileset
from hypercutter.extractors import (
    build_tileset_name_pairs,
    extract_map_layout,
    extract_map_table,
    extract_tileset,
    extract_tileset_info,
)
from hypercutter.utils import find_by_field, find_primary_from_secondary


class TestFindByField:
    def test_finds_matching_item(self):
        items = [
            Offset(address=0x1000, type=OffsetType.GLOBAL, length=0x10, name="Start"),
            Offset(address=0x2000, type=OffsetType.LOCAL, length=0x20, name="Test"),
        ]
        result = find_by_field(items, "name", "Start")
        assert result is not None
        assert result.address == 0x1000

    def test_returns_none_when_not_found(self):
        items = [
            Offset(address=0x1000, type=OffsetType.GLOBAL, length=0x10, name="Start")
        ]
        result = find_by_field(items, "name", "Missing")
        assert result is None


class TestFindPrimaryFromSecondary:
    def test_secondary_is_primary(self):
        pairs = {"Dungeon": ["Dungeon", "Interior"]}
        assert find_primary_from_secondary(pairs, "Dungeon") == "Dungeon"

    def test_secondary_in_list(self):
        pairs = {"Overworld": ["Grass", "Water"]}
        assert find_primary_from_secondary(pairs, "Water") == "Overworld"

    def test_returns_none_when_not_found(self):
        pairs = {"Overworld": ["Grass"]}
        assert find_primary_from_secondary(pairs, "Dungeon") is None


class TestExtractMapLayout:
    def test_extracts_correct_values(self):
        data = struct.pack("<iiIIII", 20, 16, 0x8000, 0x9000, 0xA000, 0xB000)
        result = extract_map_layout(data, 0)
        assert isinstance(result, MapLayout)
        assert result.width == 20
        assert result.height == 16
        assert result.border_ptr == 0x8000
        assert result.map_ptr == 0x9000
        assert result.primary_tileset_ptr == 0xA000
        assert result.secondary_tileset_ptr == 0xB000

    def test_raises_on_invalid_offset(self):
        data = bytes(0x10)
        try:
            extract_map_layout(data, 0x100)
            assert False, "Should have raised ValueError"
        except ValueError as e:
            assert "out of range" in str(e)


class TestExtractTileset:
    def test_extracts_correct_values(self):
        data = struct.pack("<??xxIIIII", 1, 0, 0x1000, 0x2000, 0x3000, 0x4000, 0x5000)
        result = extract_tileset(data, 0)
        assert isinstance(result, Tileset)
        assert result.is_compressed is True
        assert result.is_secondary is False
        assert result.tiles_ptr == 0x1000

    def test_raises_on_invalid_offset(self):
        data = bytes(0x10)
        try:
            extract_tileset(data, 0x100)
            assert False, "Should have raised ValueError"
        except ValueError as e:
            assert "out of range" in str(e)


class TestExtractMapTable:
    def test_extracts_correct_addresses(self):
        addresses = [0x8000000, 0x8000100, 0x8000200]
        data = b"".join(addr.to_bytes(4, "little") for addr in addresses)
        result = extract_map_table(data, 0, 3)
        assert result == addresses


class TestExtractTilesetInfo:
    def test_building_tries_inside_building_variant(self):
        symbols = [
            Offset(
                address=0,
                type=OffsetType.GLOBAL,
                length=0x100,
                name="gTilesetTiles_InsideBuilding",
            ),
            Offset(
                address=0,
                type=OffsetType.GLOBAL,
                length=0x200,
                name="gTilesetPalettes_InsideBuilding",
            ),
        ]
        result = extract_tileset_info("Building", symbols)
        assert result["tiles_length"] == 0x100
        assert result["palettes_len"] == 0x200

    def test_inside_building_tries_building_variant(self):
        symbols = [
            Offset(
                address=0,
                type=OffsetType.GLOBAL,
                length=0x300,
                name="gTilesetTiles_Building",
            ),
            Offset(
                address=0,
                type=OffsetType.GLOBAL,
                length=0x400,
                name="gTilesetPalettes_Building",
            ),
        ]
        result = extract_tileset_info("InsideBuilding", symbols)
        assert result["tiles_length"] == 0x300
        assert result["palettes_len"] == 0x400

    def test_returns_zero_when_not_found(self):
        symbols = []
        result = extract_tileset_info("Unknown", symbols)
        assert result["tiles_length"] == 0
        assert result["palettes_len"] == 0


class TestBuildTilesetNamePairs:
    def test_creates_correct_pairs(self):
        layouts = [
            MapLayout(
                width=20,
                height=16,
                border_ptr=0,
                map_ptr=0,
                primary_tileset_ptr=0x1000,
                secondary_tileset_ptr=0x2000,
            ),
            MapLayout(
                width=20,
                height=16,
                border_ptr=0,
                map_ptr=0,
                primary_tileset_ptr=0x1000,
                secondary_tileset_ptr=0x3000,
            ),
        ]
        symbols = [
            Offset(
                address=0x1000,
                type=OffsetType.GLOBAL,
                length=0,
                name="gTileset_Overworld",
            ),
            Offset(
                address=0x2000, type=OffsetType.GLOBAL, length=0, name="gTileset_Grass"
            ),
            Offset(
                address=0x3000, type=OffsetType.GLOBAL, length=0, name="gTileset_Water"
            ),
        ]
        result = build_tileset_name_pairs(layouts, symbols)
        assert set(result["Overworld"]) == {"Grass", "Water"}


class TestRomRevisionDetection:
    def test_detect_sym_filename_firered_v1(self):
        from hypercutter.classes import detect_sym_filename

        mock_rom = bytearray(256)
        mock_rom[0xAC:0xB0] = b"BPRE"
        mock_rom[0xB0:0xB4] = b"1.0"
        result = detect_sym_filename(bytes(mock_rom))
        assert result is None

    def test_detect_sym_filename_unknown_game(self):
        from hypercutter.classes import detect_sym_filename

        mock_rom = bytearray(256)
        mock_rom[0xAC:0xB0] = b"BPEE"
        result = detect_sym_filename(bytes(mock_rom))
        assert result is None

    def test_detect_sym_filename_too_small(self):
        from hypercutter.classes import detect_sym_filename

        result = detect_sym_filename(b"too small")
        assert result is None

    def test_compute_rom_sha256(self):
        from hypercutter.classes import compute_rom_sha256

        data = b"test data"
        sha = compute_rom_sha256(data)
        assert sha == "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9"
