"""
Extraction functions for pokeemerald ROM data.

Struct layouts (32-bit, little-endian):

MapLayout (24 bytes)
~~~~~~~~~~~~~~~~~~~~

================ ============= =====================
Field           Type          Description
================ ============= =====================
width           int32         Map width in tiles
height          int32         Map height in tiles
border_ptr      uint32        Pointer to border data
map_ptr         uint32        Pointer to map data
primary_tileset_ptr    uint32     Pointer to primary tileset
secondary_tileset_ptr  uint32     Pointer to secondary tileset
================ ============= =====================

Tileset (24 bytes)
~~~~~~~~~~~~~~~~~~

======================== ============= =======================
Field                    Type          Description
======================== ============= =======================
is_compressed            bool          Tiles are LZ77 compressed
is_secondary             bool          Is secondary tileset
tiles_ptr                uint32        Pointer to tile graphics
palettes_ptr             uint32        Pointer to palette data
metatiles_ptr            uint32        Pointer to metatile data
metatile_attributes_ptr  uint32        Pointer to metatile attributes
callback_ptr             uint32        Pointer to callback function
======================== ============= =======================
"""

import logging
import struct
from typing import Any

from .classes import MapLayout, Offset, OffsetType, Tileset
from .constants import MAP_LAYOUT_FORMAT, MAP_LAYOUT_SIZE, TILESET_FORMAT, TILESET_SIZE
from .utils import find_by_field, find_primary_from_secondary
from .lzss3 import decompress_bytes

logger = logging.getLogger(__name__)

GAME_CODE_OFFSET = 0xAC
GAME_CODE_LENGTH = 4
EXPECTED_GAME_CODE = b"BPEE"


def validate_rom(rom_data: bytes) -> bool:
    """Validate that the ROM has the expected game code."""
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
    """Parse symbol data into Offset objects."""
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
    """Load symbols from a .sym file or raw data."""
    logger.debug(f"load_symbols: {filepath_or_data}")
    if isinstance(filepath_or_data, str):
        with open(filepath_or_data, "r", encoding="utf-8") as f:
            filepath_or_data = f.read()
    elif isinstance(filepath_or_data, bytes):
        filepath_or_data = filepath_or_data.decode("utf-8")
    return _parse_symbols(filepath_or_data)


def read_rom(filepath_or_data: str | bytes) -> bytes:
    """Read a ROM file into memory."""
    if isinstance(filepath_or_data, str):
        with open(filepath_or_data, "rb") as f:
            return f.read()
    return filepath_or_data


def extract_map_layout(binary_data: bytes, offset: int) -> MapLayout:
    """Extract a MapLayout struct from binary data at the given offset."""
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


def extract_map_table(rom: bytes, map_table_sym_offset: int, count: int) -> list[int]:
    """Extract map addresses from the ROM's map table."""
    return [
        int.from_bytes(
            rom[map_table_sym_offset + i * 4 : map_table_sym_offset + (i + 1) * 4],
            "little",
        )
        for i in range(count)
    ]


def extract_tileset_info(tileset_name: str, symbols: list[Offset]) -> dict[str, int]:
    """Extract tileset tile and palette lengths from symbols."""
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


def extract_metatile_info(metatile_name: str, symbols: list[Offset]) -> int:
    """Extract metatile count from symbols."""
    sym = find_by_field(symbols, "name", f"gMetatiles_{metatile_name}")
    return sym.length if sym else 0


def build_tileset_name_pairs(
    layouts: list[MapLayout], symbols: list[Offset]
) -> dict[str, list[str]]:
    """Build a mapping of primary tilesets to their secondary tilesets."""
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


def extract_raw_data(
    binary_data: bytes,
    ptr: int,
    length: int,
    start_sym_offset: int,
    is_compressed: bool = False,
) -> bytes:
    """Extract and optionally decompress raw data from the ROM."""
    offset = ptr - start_sym_offset
    if offset < 0 or offset >= len(binary_data):
        return b""

    if is_compressed:
        try:
            return bytes(decompress_bytes(binary_data[offset:]))
        except Exception as e:
            logger.warning(f"Decompression failed at 0x{offset:X}: {e}, using raw data")
            return binary_data[offset : offset + length]
    else:
        return binary_data[offset : offset + length]


def extract_tileset_with_raw(
    rom: bytes,
    offset: int,
    start_sym_offset: int,
    tileset_info: dict[str, int],
    metatile_length: int = 0,
) -> dict[str, Any]:
    """Extract a Tileset struct and its raw data."""
    tileset = extract_tileset(rom, offset)
    from dataclasses import asdict

    data = asdict(tileset)
    data.pop("callback_ptr", None)

    tiles_len = tileset_info.get("tiles_length", 0)
    palettes_len = 512  # Standard GBA palette size

    # Extract tiles
    data["tiles_raw"] = extract_raw_data(
        rom,
        tileset.tiles_ptr,
        tiles_len,
        start_sym_offset,
        tileset.is_compressed,
    )
    data["tiles_length"] = len(data["tiles_raw"])

    # Extract palettes (16 colors * 16 palettes * 2 bytes = 512 bytes)
    data["palettes_raw"] = extract_raw_data(
        rom,
        tileset.palettes_ptr,
        palettes_len,
        start_sym_offset,
        False,  # Palettes are rarely compressed
    )
    data["palettes_length"] = len(data["palettes_raw"])

    # Extract metatiles (16 bytes per metatile)
    data["metatiles_raw"] = extract_raw_data(
        rom,
        tileset.metatiles_ptr,
        metatile_length,
        start_sym_offset,
        False,
    )
    data["metatiles_length"] = len(data["metatiles_raw"])

    return data


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
    tileset_syms = [s for s in symbols if s.name.startswith("gTileset_")]
    results = {}
    for sym in tileset_syms:
        name = sym.name.replace("gTileset_", "")
        info = extract_tileset_info(name, symbols)
        mt_len = extract_metatile_info(name, symbols)
        results[name] = extract_tileset_with_raw(
            rom, sym.address - start_sym_offset, start_sym_offset, info, mt_len
        )
    return results


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

        p_name = find_primary_from_secondary(tileset_name_pairs, mt_name)

        # If not found in pairs, try using the metatile name directly as primary
        if not p_name:
            p_name = mt_name

        primary_tileset = tilesets.get(p_name)
        if not primary_tileset:
            continue

        if p_name != mt_name:
            secondary_tileset = tilesets.get(mt_name)
            if not secondary_tileset:
                continue

            metatiles[mt_name] = {
                "primary": primary_tileset,
                "secondary": secondary_tileset,
            }
        else:
            metatiles[mt_name] = {"primary": primary_tileset, "secondary": None}

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
