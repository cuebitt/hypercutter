"""
Extraction functions for pokeemerald ROM data.

Struct layout reference (32-bit, little-endian):
- MapLayout (24 bytes): width, height, border_ptr, map_ptr, primary_tileset_ptr, secondary_tileset_ptr
- Tileset (24 bytes): is_compressed, is_secondary, tiles_ptr, palettes_ptr, metatiles_ptr, metatile_attributes_ptr, callback_ptr
"""

import logging
import struct
from typing import Any

from .classes import MapLayout, Offset, OffsetType, Tileset
from .constants import MAP_LAYOUT_FORMAT, MAP_LAYOUT_SIZE, TILESET_FORMAT, TILESET_SIZE
from .utils import find_by_field, find_primary_from_secondary

logger = logging.getLogger(__name__)

GAME_CODE_OFFSET = 0xAC
GAME_CODE_LENGTH = 4
EXPECTED_GAME_CODE = b"BPEE"


def validate_rom(rom_data: bytes) -> bool:
    """
    Validate that the ROM has the expected game code.

    Args:
        rom_data: Raw ROM bytes.

    Returns:
        True if valid or too small to validate.
    """
    if len(rom_data) < GAME_CODE_OFFSET + GAME_CODE_LENGTH:
        logger.warning("ROM file too small to validate")
        return True
    game_code = rom_data[GAME_CODE_OFFSET : GAME_CODE_OFFSET + GAME_CODE_LENGTH]
    if game_code != EXPECTED_GAME_CODE:
        logger.warning(
            f"ROM game code '{game_code.decode('latin-1')}' does not match expected '{EXPECTED_GAME_CODE.decode()}'"
        )
    return True


def _parse_symbols(data: str) -> list[Offset]:
    """Parse symbol data into Offset objects. Used by both file and bytes variants."""
    return [
        Offset(
            address=int(f"0x{line[0]}", 0),
            type=OffsetType.GLOBAL if line[1] == "g" else OffsetType.LOCAL,
            length=int(f"0x{line[2]}", 0),
            name=line[3],
        )
        for line in (x.strip().split(" ") for x in data.splitlines())
    ]


def load_symbols(filepath_or_data: str | bytes) -> list[Offset]:
    """
    Load symbols from a .sym file or raw data.

    Args:
        filepath_or_data: Path to the .sym file, or raw file contents as bytes.

    Returns:
        List of Offset objects representing each symbol.
    """
    if isinstance(filepath_or_data, str):
        with open(filepath_or_data, "r", encoding="utf-8") as f:
            filepath_or_data = f.read()
    elif isinstance(filepath_or_data, bytes):
        filepath_or_data = filepath_or_data.decode("utf-8")
    return _parse_symbols(filepath_or_data)


def read_rom(filepath_or_data: str | bytes) -> bytes:
    """
    Read a ROM file into memory.

    Args:
        filepath_or_data: Path to the .gba ROM file, or raw ROM data as bytes.

    Returns:
        Raw ROM bytes.
    """
    if isinstance(filepath_or_data, str):
        with open(filepath_or_data, "rb") as f:
            return f.read()
    return filepath_or_data


def extract_map_layout(binary_data: bytes, offset: int) -> MapLayout:
    """
    Extract a MapLayout struct from binary data at the given offset.

    Args:
        binary_data: Raw ROM bytes.
        offset: Byte offset where the struct begins.

    Returns:
        A MapLayout object with all fields populated.

    Raises:
        ValueError: If the offset is out of range.
    """
    if offset < 0 or offset + MAP_LAYOUT_SIZE > len(binary_data):
        raise ValueError(
            f"Offset 0x{offset:X} is out of range "
            f"(binary size: 0x{len(binary_data):X}, struct size: 0x{MAP_LAYOUT_SIZE:X})"
        )

    fields = struct.unpack_from(MAP_LAYOUT_FORMAT, binary_data, offset)

    return MapLayout(
        width=fields[0],
        height=fields[1],
        border_ptr=fields[2],
        map_ptr=fields[3],
        primary_tileset_ptr=fields[4],
        secondary_tileset_ptr=fields[5],
    )


def extract_tileset(binary_data: bytes, offset: int) -> Tileset:
    """
    Extract a Tileset struct from binary data at the given offset.

    Args:
        binary_data: Raw ROM bytes.
        offset: Byte offset where the struct begins.

    Returns:
        A Tileset object with all fields populated.

    Raises:
        ValueError: If the offset is out of range.
    """
    if offset < 0 or offset + TILESET_SIZE > len(binary_data):
        raise ValueError(
            f"Offset 0x{offset:X} is out of range "
            f"(binary size: 0x{len(binary_data):X}, struct size: 0x{TILESET_SIZE:X})"
        )

    fields = struct.unpack_from(TILESET_FORMAT, binary_data, offset)

    return Tileset(
        is_compressed=bool(fields[0]),
        is_secondary=bool(fields[1]),
        tiles_ptr=fields[2],
        palettes_ptr=fields[3],
        metatiles_ptr=fields[4],
        metatile_attributes_ptr=fields[5],
        callback_ptr=fields[6],
    )


def extract_map_table(rom: bytes, map_table_sym_offset: int, count: int) -> list[int]:
    """
    Extract map addresses from the ROM's map table.

    Args:
        rom: Raw ROM bytes.
        map_table_sym_offset: Offset to the start of the map table.
        count: Number of map addresses to extract.

    Returns:
        List of map addresses (as integers).
    """
    return [
        int.from_bytes(
            rom[map_table_sym_offset + i * 4 : map_table_sym_offset + (i + 1) * 4],
            "little",
        )
        for i in range(count)
    ]


def extract_tileset_info(tileset_name: str, symbols: list[Offset]) -> dict[str, int]:
    """
    Extract tileset tile and palette lengths from symbols.

    Handles the Building/InsideBuilding naming quirk by trying both variants.

    Args:
        tileset_name: Name of the tileset.
        symbols: List of symbol offsets.

    Returns:
        Dictionary with 'tiles_length' and 'palettes_len' keys.
    """
    variants = {tileset_name}
    if tileset_name == "Building":
        variants.add("InsideBuilding")
    elif tileset_name == "InsideBuilding":
        variants.add("Building")

    tiles_length = 0
    palettes_len = 0
    for variant in variants:
        tiles_sym = find_by_field(symbols, "name", f"gTilesetTiles_{variant}")
        palettes_sym = find_by_field(symbols, "name", f"gTilesetPalettes_{variant}")
        if tiles_sym:
            tiles_length = tiles_sym.length
        if palettes_sym:
            palettes_len = palettes_sym.length
        if tiles_length or palettes_len:
            break

    return {"tiles_length": tiles_length, "palettes_len": palettes_len}


def build_tileset_name_pairs(
    layouts: list[MapLayout], symbols: list[Offset]
) -> dict[str, list[str]]:
    """
    Build a mapping of primary tilesets to their secondary tilesets.

    Args:
        layouts: List of map layouts.
        symbols: List of symbol offsets.

    Returns:
        Dictionary mapping primary tileset names to lists of secondary tileset names.
    """
    tileset_pairs: dict[int, set[int]] = {}
    for layout in layouts:
        if layout.primary_tileset_ptr not in tileset_pairs:
            tileset_pairs[layout.primary_tileset_ptr] = set()
        tileset_pairs[layout.primary_tileset_ptr].add(layout.secondary_tileset_ptr)

    tileset_name_pairs: dict[str, list[str]] = {}
    for primary, secondary_set in tileset_pairs.items():
        primary_sym = find_by_field(symbols, "address", primary)
        if not primary_sym:
            continue

        secondary_names = []
        for addr in secondary_set:
            secondary_sym = find_by_field(symbols, "address", addr)
            if secondary_sym:
                secondary_names.append(secondary_sym.name.replace("gTileset_", ""))

        tileset_name_pairs[primary_sym.name.replace("gTileset_", "")] = list(
            set(secondary_names)
        )

    return tileset_name_pairs


def extract_all_tilesets(
    symbols: list[Offset], rom: bytes, start_sym_offset: int
) -> dict[str, dict[str, Any]]:
    """
    Extract all tilesets from the ROM.

    Args:
        symbols: List of symbol offsets.
        rom: Raw ROM bytes.
        start_sym_offset: Address of the 'Start' symbol.

    Returns:
        Dictionary mapping tileset names to their extracted data.
    """
    from dataclasses import asdict

    tileset_syms = [s for s in symbols if s.name.startswith("gTileset_")]
    return {
        sym.name.replace("gTileset_", ""): asdict(
            extract_tileset(rom, sym.address - start_sym_offset)
        )
        for sym in tileset_syms
    }


def extract_metatiles(
    tileset_name_pairs: dict[str, list[str]],
    tilesets: dict[str, dict[str, Any]],
    symbols: list[Offset],
) -> dict[str, dict[str, Any]]:
    """
    Extract all metatiles with their associated tileset information.

    Args:
        tileset_name_pairs: Mapping of primary to secondary tilesets.
        tilesets: Extracted tileset data.
        symbols: List of symbol offsets.

    Returns:
        Dictionary mapping metatile names to their tileset data.
    """
    metatile_syms = [s for s in symbols if s.name.startswith("gMetatiles_")]
    metatiles: dict[str, dict[str, Any]] = {}

    for sym in metatile_syms:
        mt_name = sym.name.replace("gMetatiles_", "")
        metatiles[mt_name] = {"primary": None, "secondary": None}

        p_name = find_primary_from_secondary(tileset_name_pairs, mt_name)
        if not p_name:
            continue

        if p_name != mt_name:
            primary_tileset = tilesets[p_name]
            primary_tileset["name"] = p_name
            primary_tileset.update(extract_tileset_info(p_name, symbols))

            secondary_tileset = tilesets[mt_name]
            secondary_tileset["name"] = mt_name
            secondary_tileset.update(extract_tileset_info(mt_name, symbols))

            metatiles[mt_name] = {
                "primary": primary_tileset,
                "secondary": secondary_tileset,
            }
        else:
            tileset = tilesets[mt_name]
            tileset["name"] = mt_name
            metatiles[mt_name] = {"primary": tileset, "secondary": None}

    for mt in metatiles.values():
        if mt["primary"]:
            mt["primary"].pop("callback_ptr", None)
            mt["primary"].pop("metatile_attributes_ptr", None)
        if mt["secondary"]:
            mt["secondary"].pop("callback_ptr", None)
            mt["secondary"].pop("metatile_attributes_ptr", None)

    return metatiles


def extract(sym_data: str | bytes, rom_data: str | bytes) -> dict[str, Any]:
    """
    Extract metatiles from a pokeemerald ROM.

    Args:
        sym_data: Path to the .sym file, or raw .sym contents as bytes.
        rom_data: Path to the .gba ROM file, or raw ROM data as bytes.

    Returns:
        Dictionary mapping metatile names to their tileset data.
    """
    symbols = load_symbols(sym_data)
    rom = read_rom(rom_data)
    validate_rom(rom)

    start_sym = find_by_field(symbols, "name", "Start")
    if not start_sym:
        raise ValueError("Symbol 'Start' not found in symbols file")
    start_sym_offset = start_sym.address

    map_table_sym = find_by_field(symbols, "name", "gMapLayouts")
    if not map_table_sym:
        raise ValueError("Symbol 'gMapLayouts' not found in symbols file")
    map_table_sym_offset = map_table_sym.address

    map_table_sym_idx = symbols.index(map_table_sym)
    map_table_length = symbols[map_table_sym_idx + 1].address - map_table_sym_offset
    map_table_count = map_table_length // 0x4

    rel_offset = map_table_sym_offset - start_sym_offset
    logger.info("Found %d maps at 0x%x", map_table_count, rel_offset)

    map_table = extract_map_table(rom, rel_offset, map_table_count)
    layouts = [extract_map_layout(rom, addr - start_sym_offset) for addr in map_table]

    tileset_name_pairs = build_tileset_name_pairs(layouts, symbols)
    tilesets = extract_all_tilesets(symbols, rom, start_sym_offset)
    metatiles = extract_metatiles(tileset_name_pairs, tilesets, symbols)

    return metatiles
