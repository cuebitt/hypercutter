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
from .constants import (
    MAP_LAYOUT_FORMAT,
    MAP_LAYOUT_SIZE,
    PALETTE_SIZE,
    TILESET_FORMAT,
    TILESET_SIZE,
)
from .utils import find_by_field, find_primary_from_secondary
from .lzss3 import decompress_bytes

logger = logging.getLogger(__name__)

# ROM header location and expected game code for Pokemon Emerald
GAME_CODE_OFFSET = 0xAC
GAME_CODE_LENGTH = 4
EXPECTED_GAME_CODE = b"BPEE"


def validate_rom(rom_data: bytes) -> bool:
    """
    Validate that the ROM has the expected game code.

    Pokemon Emerald ROMs have the game code "BPEE" at offset 0xAC.
    This helps verify we're working with the correct ROM file.

    Args:
        rom_data: Raw ROM bytes.

    Returns:
        True if validation passed (even with warnings).
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
    """
    Parse symbol data into Offset objects.

    Symbol file format (one symbol per line):
    08abcde0 l 00000010 gSymbolName
    [address] [type] [length] [name]

    Args:
        data: Raw symbol file contents.

    Returns:
        List of Offset objects.
    """
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
    """
    Extract a MapLayout struct from binary data at the given offset.

    MapLayout defines the structure of a game map including its dimensions
    and which tilesets it uses.

    Args:
        binary_data: Raw ROM bytes.
        offset: Byte offset where the MapLayout struct begins.

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


def extract_map_table(rom: bytes, map_table_sym_offset: int, count: int) -> list[int]:
    """
    Extract map addresses from the ROM's map table.

    The map table is an array of pointers to MapLayout structs.
    Each pointer is a 32-bit ROM address.

    Args:
        rom: Raw ROM bytes.
        map_table_sym_offset: File offset where the map table starts.
        count: Number of map entries to extract.

    Returns:
        List of ROM addresses pointing to MapLayout structs.
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

    Symbol files contain the sizes of tiles and palette data.
    We look for gTilesetTiles_{name} and gTilesetPalettes_{name} symbols.

    Some tilesets have variant names (e.g., Building/InsideBuilding), so we
    try both if the primary name doesn't match.

    Args:
        tileset_name: Name of the tileset (e.g., "Overworld", "Building").
        symbols: List of symbol offsets from the .sym file.

    Returns:
        Dictionary with 'tiles_length' and 'palettes_len' values.
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


def extract_metatile_info(metatile_name: str, symbols: list[Offset]) -> int:
    """
    Extract metatile data length from symbols.

    Looks for gMetatiles_{name} symbol which contains the length
    of the metatile data in bytes.

    Args:
        metatile_name: Name of the metatile (usually matches tileset name).
        symbols: List of symbol offsets from the .sym file.

    Returns:
        Length of metatile data in bytes, or 0 if not found.
    """
    sym = find_by_field(symbols, "name", f"gMetatiles_{metatile_name}")
    return sym.length if sym else 0


def build_tileset_name_pairs(
    layouts: list[MapLayout], symbols: list[Offset]
) -> dict[str, list[str]]:
    """
    Build a mapping of primary tilesets to their secondary tilesets.

    Each map uses a primary tileset (always loaded) and optionally a
    secondary tileset (specific to that map). This function maps which
    secondary tilesets belong to which primary.

    Args:
        layouts: List of extracted MapLayout objects.
        symbols: List of symbol offsets for name lookups.

    Returns:
        Dictionary mapping primary tileset name to list of secondary names.
        Example: {"Overworld": ["Grass", "Water", "Building"]}
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
    """
    Extract and optionally decompress raw data from the ROM.

    GBA ROMs use LZ77 compression for many assets. This function reads
    data from a ROM address and optionally decompresses it.

    Args:
        binary_data: Raw ROM bytes.
        ptr: ROM address where data starts.
        length: Expected length (for uncompressed) or max length (for compressed).
        start_sym_offset: Base address for ROM pointers (typically 0x8000000).
        is_compressed: Whether to decompress the data using LZ77.

    Returns:
        Extracted (and possibly decompressed) data bytes.
    """
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
    """
    Extract a Tileset struct and its raw data.

    This is the main extraction function for a single tileset. It reads:
    - Tile graphics (may be LZ77 compressed)
    - Palette data (16 palettes × 16 colors × 2 bytes)
    - Metatile definitions (16 bytes per metatile)

    Args:
        rom: Raw ROM bytes.
        offset: File offset where the Tileset struct begins.
        start_sym_offset: Base address for ROM pointers.
        tileset_info: Dictionary with 'tiles_length' and 'palettes_len'.
        metatile_length: Length of metatile data in bytes.

    Returns:
        Dictionary containing the Tileset struct plus raw data fields.
    """
    tileset = extract_tileset(rom, offset)
    from dataclasses import asdict

    data = asdict(tileset)
    data.pop("callback_ptr", None)

    tiles_len = tileset_info.get("tiles_length", 0)
    palettes_len = PALETTE_SIZE

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

    map_table_sym_idx = symbols.index(map_table_sym)
    map_table_length = symbols[map_table_sym_idx + 1].address - map_table_sym.address
    map_table_count = map_table_length // 4

    rel_offset = map_table_sym.address - start_sym_offset
    logger.info("Found %d maps at 0x%x", map_table_count, rel_offset)

    map_table = extract_map_table(rom, rel_offset, map_table_count)
    layouts = [extract_map_layout(rom, addr - start_sym_offset) for addr in map_table]

    logger.debug("Building tileset name pairs")
    tileset_name_pairs = build_tileset_name_pairs(layouts, symbols)
    logger.debug("Extracting %d tilesets", len(tileset_name_pairs))
    tilesets = extract_all_tilesets(symbols, rom, start_sym_offset)
    logger.debug("Extracting metatiles")
    metatiles = extract_metatiles(tileset_name_pairs, tilesets, symbols)

    logger.info("Extraction complete: %d metatiles", len(metatiles))
    return metatiles, start_sym_offset
