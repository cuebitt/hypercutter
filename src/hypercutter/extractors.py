"""
Extraction functions for GBA Pokemon ROM data.

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
from pathlib import Path
from typing import Any

from .classes import (
    GAME_CODE_OFFSET,
    GAME_CODE_LENGTH,
    MapLayout,
    MonCoords,
    Offset,
    OffsetType,
    SpritePalette,
    SpriteSheet,
    SUPPORTED_GAMES,
    Tileset,
)
from .constants import (
    MAP_LAYOUT_FORMAT,
    MAP_LAYOUT_SIZE,
    MAX_DECOMPRESS_READ_SIZE,
    MON_COORDS_ENTRY_SIZE,
    PALETTE_SIZE,
    SPRITE_PALETTE_ENTRY_SIZE,
    SPRITE_SHEET_ENTRY_SIZE,
    TILESET_FORMAT,
    TILESET_SIZE,
)
from .utils import build_field_index, find_by_field, find_primary_from_secondary
from .lzss3 import decompress_bytes

logger = logging.getLogger(__name__)

__all__ = [
    "validate_rom",
    "load_symbols",
    "read_rom",
    "extract_map_layout",
    "extract_map_table",
    "extract_tileset_info",
    "extract_metatile_info",
    "build_tileset_name_pairs",
    "extract_tileset",
    "extract_raw_data",
    "extract_tileset_with_raw",
    "extract_all_tilesets",
    "extract_metatiles",
    "extract",
    # Sprite extraction functions
    "extract_sprite_sheet",
    "extract_sprite_palette",
    "extract_mon_coords",
    "extract_sprite_table",
    "extract_sprite_data",
    "extract_palette_data",
    "extract_all_pokemon_sprites",
]


def validate_rom(rom_data: bytes, expected_game_code: bytes | None = None) -> bool:
    """
    Validate that the ROM has a supported game code.

    Args:
        rom_data: Raw ROM bytes.
        expected_game_code: Optional specific game code to validate against.

    Returns:
        True if validation passed (even with warnings).
    """
    if len(rom_data) < GAME_CODE_OFFSET + GAME_CODE_LENGTH:
        logger.warning("ROM file too small to validate")
        return True
    game_code = rom_data[GAME_CODE_OFFSET : GAME_CODE_OFFSET + GAME_CODE_LENGTH]
    if expected_game_code is not None and game_code != expected_game_code:
        logger.warning(
            f"ROM game code '{game_code.decode('latin-1')}' does not match expected '{expected_game_code.decode()}'"
        )
    elif game_code not in SUPPORTED_GAMES:
        logger.warning(
            f"ROM game code '{game_code.decode('latin-1')}' is not a supported game"
        )
    return True


def _parse_symbols(data: str) -> list[Offset]:
    """
    Parse symbol data into Offset objects.

    Symbol file format (one symbol per line):
    08abcde0 l 00000010 gSymbolName
    [address] [type] [length] [name]

    If length is 0 or missing, calculate it using the next symbol's address.
    For gTilesetTiles_* symbols, prefer using the matching gTilesetPalettes_* symbol.

    Args:
        data: Raw symbol file contents.

    Returns:
        List of Offset objects.
    """
    lines = [x.strip().split(" ") for x in data.splitlines() if x.strip()]
    offsets = []

    # Build a map of symbol names to addresses for quick lookup
    name_to_addr = {}
    for line in lines:
        if len(line) >= 4 and line[0] and line[3]:
            try:
                addr = int(f"0x{line[0]}", 0)
                name_to_addr[line[3]] = addr
            except ValueError:
                pass

    for i, line in enumerate(lines):
        if len(line) < 4 or not line[0]:
            continue

        address = int(f"0x{line[0]}", 0)
        sym_type = OffsetType.GLOBAL if line[1] == "g" else OffsetType.LOCAL
        length = int(f"0x{line[2]}", 0)
        name = line[3]

        # If still 0, use next symbol
        if length == 0 and i + 1 < len(lines):
            next_line = lines[i + 1]
            if len(next_line) >= 1 and next_line[0]:
                try:
                    next_address = int(f"0x{next_line[0]}", 0)
                    if next_address > address:
                        length = next_address - address
                except ValueError:
                    pass

        offsets.append(
            Offset(
                address=address,
                scope=sym_type,
                length=length,
                name=name,
            )
        )

    return offsets


def load_symbols(filepath_or_data: str | bytes) -> list[Offset]:
    """Load symbols from a .sym file or raw data."""
    logger.debug("load_symbols: %r", filepath_or_data)
    if isinstance(filepath_or_data, str) and "\n" not in filepath_or_data and Path(filepath_or_data).is_file():
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
    fmt = f"<{count}I"
    return list(struct.unpack_from(fmt, rom, map_table_sym_offset))


def extract_tileset_info(
    tileset_name: str,
    symbols: list[Offset],
    name_index: dict[str, Offset] | None = None,
) -> dict[str, int]:
    """
    Extract tileset tile and palette lengths from symbols.

    Symbol files contain the sizes of tiles and palette data.
    We look for gTilesetTiles_{name} and gTilesetPalettes_{name} symbols.

    Some tilesets have variant names (e.g., Building/InsideBuilding), so we
    try both if the primary name doesn't match.

    Args:
        tileset_name: Name of the tileset (e.g., "Overworld", "Building").
        symbols: List of symbol offsets from the .sym file.
        name_index: Optional pre-built name-to-Offset dict for O(1) lookups.

    Returns:
        Dictionary with 'tiles_length' and 'palettes_len' values.
    """
    if name_index is None:
        name_index = build_field_index(symbols, "name")

    variants = {tileset_name}
    if tileset_name == "Building":
        variants.add("InsideBuilding")
    elif tileset_name == "InsideBuilding":
        variants.add("Building")

    tiles_length = 0
    palettes_len = 0
    for variant in variants:
        tiles_sym = name_index.get(f"gTilesetTiles_{variant}")
        palettes_sym = name_index.get(f"gTilesetPalettes_{variant}")
        if tiles_sym:
            tiles_length = tiles_sym.length
        if palettes_sym:
            palettes_len = palettes_sym.length
        if tiles_length or palettes_len:
            break

    return {"tiles_length": tiles_length, "palettes_len": palettes_len}


def extract_metatile_info(
    metatile_name: str,
    symbols: list[Offset],
    name_index: dict[str, Offset] | None = None,
) -> int:
    """
    Extract metatile data length from symbols.

    Looks for gMetatiles_{name} symbol which contains the length
    of the metatile data in bytes.

    Args:
        metatile_name: Name of the metatile (usually matches tileset name).
        symbols: List of symbol offsets from the .sym file.
        name_index: Optional pre-built name-to-Offset dict for O(1) lookups.

    Returns:
        Length of metatile data in bytes, or 0 if not found.
    """
    if name_index is None:
        name_index = build_field_index(symbols, "name")
    sym = name_index.get(f"gMetatiles_{metatile_name}")
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
    addr_index = build_field_index(symbols, "address")

    tileset_pairs: dict[int, set[int]] = {}
    for layout in layouts:
        if layout.primary_tileset_ptr not in tileset_pairs:
            tileset_pairs[layout.primary_tileset_ptr] = set()
        tileset_pairs[layout.primary_tileset_ptr].add(layout.secondary_tileset_ptr)

    tileset_name_pairs: dict[str, list[str]] = {}
    for primary, secondary_set in tileset_pairs.items():
        primary_sym = addr_index.get(primary)
        if not primary_sym:
            continue

        secondary_names = []
        for addr in secondary_set:
            secondary_sym = addr_index.get(addr)
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
        # Check for valid LZ77 compression header (0x10 or 0x11)
        if length > 0 and offset + 1 < len(binary_data):
            header = binary_data[offset]
            if header not in (0x10, 0x11):
                logger.debug(
                    f"Data at 0x{offset:X} doesn't have LZ77 header, using raw"
                )
                return binary_data[offset : offset + length]
        try:
            # Read a reasonable maximum for decompression
            # The calculated length may be too small for compressed data
            max_read = min(MAX_DECOMPRESS_READ_SIZE, len(binary_data) - offset)
            compressed_data = binary_data[offset : offset + max_read]
            result = decompress_bytes(compressed_data)
            return bytes(result)
        except Exception as e:
            logger.warning(f"Decompression failed at 0x{offset:X}: {e}, using raw data")
            return binary_data[offset : offset + length] if length > 0 else b""
    else:
        return binary_data[offset : offset + length] if length > 0 else b""


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
    name_index = build_field_index(symbols, "name")
    tileset_syms = [s for s in symbols if s.name.startswith("gTileset_")]
    results = {}
    for sym in tileset_syms:
        name = sym.name.replace("gTileset_", "")
        info = extract_tileset_info(name, symbols, name_index)
        mt_len = extract_metatile_info(name, symbols, name_index)
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


def extract(
    sym_data: str | bytes, rom_data: str | bytes, validate: bool = True
) -> tuple[dict[str, Any], int]:
    """
    Extract metatiles from a GBA Pokemon ROM.

    Args:
        sym_data: Path to the .sym file, or raw .sym contents as bytes.
        rom_data: Path to the .gba ROM file, or raw ROM data as bytes.
        validate: Whether to validate the ROM game code. Defaults to True.

    Returns:
        Tuple of (metatiles dict, start_sym_offset).
    """
    symbols = load_symbols(sym_data)
    rom = read_rom(rom_data)
    if validate:
        validate_rom(rom)

    start_sym = find_by_field(symbols, "name", "Start")
    if not start_sym:
        raise ValueError("Symbol 'Start' not found in symbols file")
    start_sym_offset = start_sym.address

    map_table_sym = find_by_field(symbols, "name", "gMapLayouts")
    if not map_table_sym:
        raise ValueError("Symbol 'gMapLayouts' not found in symbols file")

    map_table_length = map_table_sym.length
    map_table_count = map_table_length // 4

    rel_offset = map_table_sym.address - start_sym_offset
    logger.info("Found %d maps at 0x%x", map_table_count, rel_offset)

    map_table = extract_map_table(rom, rel_offset, map_table_count)
    layouts_to_extract = list(filter(lambda x: x > start_sym_offset, map_table))
    layouts = [
        extract_map_layout(rom, addr - start_sym_offset) for addr in layouts_to_extract
    ]

    logger.debug("Building tileset name pairs")
    tileset_name_pairs = build_tileset_name_pairs(layouts, symbols)
    logger.debug("Extracting %d tilesets", len(tileset_name_pairs))
    tilesets = extract_all_tilesets(symbols, rom, start_sym_offset)
    logger.debug("Extracting metatiles")
    metatiles = extract_metatiles(tileset_name_pairs, tilesets, symbols)

    logger.info("Extraction complete: %d metatiles", len(metatiles))
    return metatiles, start_sym_offset


def extract_sprite_sheet(
    rom: bytes,
    offset: int,
    start_sym_offset: int,
) -> SpriteSheet:
    """
    Extract a CompressedSpriteSheet from ROM.

    CompressedSpriteSheet struct (8 bytes):
    - data_ptr (4 bytes): ROM address of compressed tile data
    - size (2 bytes): Uncompressed size in bytes
    - tag (2 bytes): Species index

    Args:
        rom: Raw ROM bytes.
        offset: File offset where the struct begins.
        start_sym_offset: Base address for ROM pointers.

    Returns:
        A SpriteSheet object.
    """
    if offset < 0 or offset + SPRITE_SHEET_ENTRY_SIZE > len(rom):
        raise ValueError(
            f"Offset 0x{offset:X} is out of range "
            f"(ROM size: 0x{len(rom):X}, struct size: 0x{SPRITE_SHEET_ENTRY_SIZE:X})"
        )

    fields = struct.unpack_from("<IHH", rom, offset)
    return SpriteSheet(
        data_ptr=fields[0],
        size=fields[1],
        tag=fields[2],
    )


def extract_sprite_palette(
    rom: bytes,
    offset: int,
    start_sym_offset: int,
) -> SpritePalette:
    """
    Extract a CompressedSpritePalette from ROM.

    CompressedSpritePalette struct (8 bytes):
    - data_ptr (4 bytes): ROM address of compressed palette data
    - tag (2 bytes): Species index
    - padding (2 bytes): Unused

    Args:
        rom: Raw ROM bytes.
        offset: File offset where the struct begins.
        start_sym_offset: Base address for ROM pointers.

    Returns:
        A SpritePalette object.
    """
    if offset < 0 or offset + SPRITE_PALETTE_ENTRY_SIZE > len(rom):
        raise ValueError(
            f"Offset 0x{offset:X} is out of range "
            f"(ROM size: 0x{len(rom):X}, struct size: 0x{SPRITE_PALETTE_ENTRY_SIZE:X})"
        )

    fields = struct.unpack_from("<IHH", rom, offset)
    return SpritePalette(
        data_ptr=fields[0],
        tag=fields[1],
    )


def extract_mon_coords(
    rom: bytes,
    offset: int,
) -> MonCoords:
    """
    Extract MonCoords from ROM.

    MonCoords struct (4 bytes in ROM, padded):
    - size (1 byte): Packed width (high nibble) and height (low nibble) in tiles
    - y_offset (1 byte): Pixels from bottom of sprite to bottom of 64x64 frame
    - padding (2 bytes): Unused alignment padding

    Args:
        rom: Raw ROM bytes.
        offset: File offset where the struct begins.

    Returns:
        A MonCoords object.
    """
    if offset < 0 or offset + MON_COORDS_ENTRY_SIZE > len(rom):
        raise ValueError(
            f"Offset 0x{offset:X} is out of range "
            f"(ROM size: 0x{len(rom):X}, struct size: 0x{MON_COORDS_ENTRY_SIZE:X})"
        )

    # Only read the first 2 bytes (size and y_offset), skip 2 padding bytes
    fields = struct.unpack_from("<BB", rom, offset)
    return MonCoords(
        size=fields[0],
        y_offset=fields[1],
    )


def extract_sprite_table(
    rom: bytes,
    table_offset: int,
    count: int,
    start_sym_offset: int,
) -> list[SpriteSheet]:
    """
    Extract all entries from a sprite table.

    Args:
        rom: Raw ROM bytes.
        table_offset: File offset where the table begins.
        count: Number of entries to extract.
        start_sym_offset: Base address for ROM pointers.

    Returns:
        List of SpriteSheet objects.
    """
    sprites = []
    for i in range(count):
        offset = table_offset + (i * SPRITE_SHEET_ENTRY_SIZE)
        sprites.append(extract_sprite_sheet(rom, offset, start_sym_offset))
    return sprites


def extract_sprite_data(
    rom: bytes,
    sprite: SpriteSheet,
    start_sym_offset: int,
    is_compressed: bool = True,
) -> bytes:
    """
    Extract and decompress sprite tile data.

    Args:
        rom: Raw ROM bytes.
        sprite: SpriteSheet containing the pointer to tile data.
        start_sym_offset: Base address for ROM pointers.
        is_compressed: Whether the data is LZ77 compressed.

    Returns:
        Decompressed tile data bytes, truncated to expected size.
    """
    data = extract_raw_data(
        rom,
        sprite.data_ptr,
        sprite.size,
        start_sym_offset,
        is_compressed,
    )
    # Truncate to expected size: LZ77 may decompress extra garbage
    if is_compressed and len(data) > sprite.size:
        data = data[:sprite.size]
    return data


def extract_palette_data(
    rom: bytes,
    palette: SpritePalette,
    start_sym_offset: int,
    is_compressed: bool = True,
) -> bytes:
    """
    Extract and decompress palette data.

    Args:
        rom: Raw ROM bytes.
        palette: SpritePalette containing the pointer to palette data.
        start_sym_offset: Base address for ROM pointers.
        is_compressed: Whether the data is LZ77 compressed.

    Returns:
        Decompressed palette data bytes (typically 32 bytes for 16 colors).
    """
    # Palette size is typically 32 bytes (16 colors * 2 bytes each)
    # We use a max read size since compressed data may be smaller
    return extract_raw_data(
        rom,
        palette.data_ptr,
        32,
        start_sym_offset,
        is_compressed,
    )


def extract_all_pokemon_sprites(
    rom: bytes,
    start_sym_offset: int,
    symbols: list[Offset],
) -> dict[int, dict[str, Any]]:
    """
    Extract all Pokemon sprites organized by species ID.

    Looks up sprite table addresses from the symbols file and derives
    the pokemon count from the table sizes.

    Args:
        rom: Raw ROM bytes.
        start_sym_offset: Base address for ROM pointers.
        symbols: List of symbol offsets from the .sym file.

    Returns:
        Dictionary mapping species ID to sprite data.
    """
    from .constants import (
        MON_COORDS_ENTRY_SIZE,
        SPRITE_PALETTE_ENTRY_SIZE,
        SPRITE_SHEET_ENTRY_SIZE,
        SPRITE_SYMBOL_NAMES,
    )

    # Look up sprite table addresses from symbols
    name_index = build_field_index(symbols, "name")
    tables: dict[str, int] = {}
    for sym_name in SPRITE_SYMBOL_NAMES:
        sym = name_index.get(sym_name)
        if sym is None:
            raise ValueError(f"Symbol '{sym_name}' not found in symbols file")
        tables[sym_name] = sym.address

    # Derive pokemon count from the front pic table size
    front_table_sym = name_index["gMonFrontPicTable"]
    pokemon_count = front_table_sym.length // SPRITE_SHEET_ENTRY_SIZE

    front_table_offset = tables["gMonFrontPicTable"] - start_sym_offset
    back_table_offset = tables["gMonBackPicTable"] - start_sym_offset
    palette_table_offset = tables["gMonPaletteTable"] - start_sym_offset
    shiny_palette_table_offset = tables["gMonShinyPaletteTable"] - start_sym_offset
    front_coords_offset = tables["gMonFrontPicCoords"] - start_sym_offset
    back_coords_offset = tables["gMonBackPicCoords"] - start_sym_offset

    logger.info("Extracting %d Pokemon sprites", pokemon_count)

    results: dict[int, dict[str, Any]] = {}

    for species_id in range(0, pokemon_count):
        idx = species_id  # Tables are 1-indexed (index 0 is SPECIES_NONE)

        # Extract table entries
        front_offset = front_table_offset + (idx * SPRITE_SHEET_ENTRY_SIZE)
        back_offset = back_table_offset + (idx * SPRITE_SHEET_ENTRY_SIZE)
        palette_offset = palette_table_offset + (idx * SPRITE_PALETTE_ENTRY_SIZE)
        shiny_palette_offset = shiny_palette_table_offset + (
            idx * SPRITE_PALETTE_ENTRY_SIZE
        )
        front_coords_idx = front_coords_offset + (idx * MON_COORDS_ENTRY_SIZE)
        back_coords_idx = back_coords_offset + (idx * MON_COORDS_ENTRY_SIZE)

        try:
            front = extract_sprite_sheet(rom, front_offset, start_sym_offset)
            back = extract_sprite_sheet(rom, back_offset, start_sym_offset)

            # Skip entries with no sprite data
            if front.data_ptr == 0 and back.data_ptr == 0:
                continue

            # Skip Old Unown placeholders (beta entries with no real sprites)
            if species_id >= 252 and species_id <= 276:
                continue

            palette = extract_sprite_palette(rom, palette_offset, start_sym_offset)
            shiny_palette = extract_sprite_palette(
                rom, shiny_palette_offset, start_sym_offset
            )
            front_coords = extract_mon_coords(rom, front_coords_idx)
            back_coords = extract_mon_coords(rom, back_coords_idx)

            # Extract raw data
            front_tile_data = extract_sprite_data(rom, front, start_sym_offset)
            back_tile_data = extract_sprite_data(rom, back, start_sym_offset)
            palette_data = extract_palette_data(rom, palette, start_sym_offset)
            shiny_palette_data = extract_palette_data(
                rom, shiny_palette, start_sym_offset
            )

            results[species_id] = {
                "front": front,
                "back": back,
                "palette": palette,
                "shiny_palette": shiny_palette,
                "front_coords": front_coords,
                "back_coords": back_coords,
                "front_tile_data": front_tile_data,
                "back_tile_data": back_tile_data,
                "palette_data": palette_data,
                "shiny_palette_data": shiny_palette_data,
            }
        except ValueError as e:
            logger.warning("Failed to extract sprite for species %d: %s", species_id, e)
            continue

    logger.info("Extracted %d Pokemon sprites", len(results))
    return results


def _parse_form_symbol_name(name: str) -> tuple[str, str] | None:
    """
    Parse a form sprite symbol name into (species, form).

    Examples:
        "gMonFrontPic_UnownB" -> ("Unown", "B")
        "gMonFrontPic_Castform" -> ("Castform", "Normal")
        "gMonBackPic_UnownExclamationMark" -> ("Unown", "!")
    """
    # Strip prefix
    for prefix in ("gMonFrontPic_", "gMonBackPic_"):
        if name.startswith(prefix):
            species_form = name[len(prefix):]
            break
    else:
        return None

    # Known form suffixes for multi-form species
    form_map = {
        "Unown": {
            "A": "A", "B": "B", "C": "C", "D": "D", "E": "E",
            "F": "F", "G": "G", "H": "H", "I": "I", "J": "J",
            "K": "K", "L": "L", "M": "M", "N": "N", "O": "O",
            "P": "P", "Q": "Q", "R": "R", "S": "S", "T": "T",
            "U": "U", "V": "V", "W": "W", "X": "X", "Y": "Y",
            "Z": "Z",
            "ExclamationMark": "!",
            "QuestionMark": "?",
        },
        "Castform": {
            "Castform": "Normal",
            "CastformRain": "Rain",
            "CastformSunny": "Sunny",
            "CastformSnow": "Snow",
        },
        "Deoxys": {
            "Deoxys": "Normal",
            "DeoxysAttack": "Attack",
            "DeoxysDefense": "Defense",
            "DeoxysSpeed": "Speed",
        },
        "Burmy": {
            "Burmy": "Plant",
            "BurmySandy": "Sandy",
            "BurmyTrash": "Trash",
        },
        "Wormadam": {
            "Wormadam": "Plant",
            "WormadamSandy": "Sandy",
            "WormadamTrash": "Trash",
        },
        "Shellos": {
            "Shellos": "West",
            "ShellosEast": "East",
        },
        "Gastrodon": {
            "Gastrodon": "West",
            "GastrodonEast": "East",
        },
        "Rotom": {
            "Rotom": "Normal",
            "RotomHeat": "Heat",
            "RotomWash": "Wash",
            "RotomFrost": "Frost",
            "RotomFan": "Fan",
            "RotomMow": "Mow",
        },
        "Giratina": {
            "Giratina": "Altered",
            "GiratinaOrigin": "Origin",
        },
        "Shaymin": {
            "Shaymin": "Land",
            "ShayminSky": "Sky",
        },
    }

    # Try to match known species + form
    for species, forms in form_map.items():
        if species_form == species:
            # This is the base species, not a form - skip it
            return None
        # Check if species_form ends with a known form suffix
        for form_suffix, form_name in forms.items():
            if species_form.endswith(form_suffix) and species_form != form_suffix:
                return (species, form_name)
            if species_form == form_suffix:
                return (species, form_name)

    # Not a recognized form - skip it
    return None


def extract_pokemon_form_sprites(
    rom: bytes,
    start_sym_offset: int,
    symbols: list[Offset],
) -> dict[str, dict[str, Any]]:
    """
    Extract alternate form sprites from individual ROM symbols.

    Handles both individual form symbols (Unown) and packed form data
    (Castform, Deoxys where all forms are concatenated in one symbol).

    Returns:
        Dictionary mapping "Species_Form" to sprite data.
    """
    from .classes import GAME_CODE_OFFSET, SUPPORTED_GAMES

    results: dict[str, dict[str, Any]] = {}
    name_index = build_field_index(symbols, "name")
    FORM_SIZE = 2048  # 64x64 at 4bpp = 2048 bytes per form

    # Detect game from ROM header for game-specific form naming
    deoxys_alt = "Alternate"
    if len(rom) >= GAME_CODE_OFFSET + 4:
        game_code = rom[GAME_CODE_OFFSET : GAME_CODE_OFFSET + 4]
        game_profile = SUPPORTED_GAMES.get(game_code)
        if game_profile:
            alt_forms: dict[str, str] = {
                "emerald": "Speed",
                "firered": "Attack",
                "leafgreen": "Defense",
            }
            deoxys_alt = alt_forms.get(game_profile.short_name, "Alternate")
            if game_profile.short_name in ("ruby", "sapphire"):
                # Ruby/Sapphire: only one form, no packed alternate
                deoxys_alt = ""

    # Species with packed form data (all forms concatenated in one symbol)
    packed_form_species = {
        "Castform": ["Normal", "Rain", "Sunny", "Snow"],
        "Deoxys": ["Normal"] + ([deoxys_alt] if deoxys_alt else []),
    }

    # Extract individual form symbols (e.g., Unown letters)
    for sym in symbols:
        if not (sym.name.startswith("gMonFrontPic_") or sym.name.startswith("gMonBackPic_")):
            continue

        parsed = _parse_form_symbol_name(sym.name)
        if parsed is None:
            continue

        species, form = parsed
        key = f"{species}_{form}" if form else species

        if key not in results:
            results[key] = {
                "species": species,
                "form": form,
                "front_tile_data": b"",
                "back_tile_data": b"",
                "palette_data": b"",
                "shiny_palette_data": b"",
            }

        # Extract the sprite data
        data_offset = sym.address - start_sym_offset
        compressed = rom[data_offset : data_offset + min(sym.length, 0x10000)]
        try:
            decompressed = bytes(decompress_bytes(compressed))
        except Exception:
            continue

        if sym.name.startswith("gMonFrontPic_"):
            # Truncate to expected size (LZ77 may decompress extra garbage)
            if len(decompressed) > FORM_SIZE:
                decompressed = decompressed[:FORM_SIZE]
            results[key]["front_tile_data"] = decompressed
        elif sym.name.startswith("gMonBackPic_"):
            if len(decompressed) > FORM_SIZE:
                decompressed = decompressed[:FORM_SIZE]
            results[key]["back_tile_data"] = decompressed

    # Extract packed form data for species like Castform and Deoxys

    for species, form_names in packed_form_species.items():
        front_sym_name = f"gMonFrontPic_{species}"
        back_sym_name = f"gMonBackPic_{species}"

        front_sym = name_index.get(front_sym_name)
        back_sym = name_index.get(back_sym_name)

        if front_sym is None:
            continue

        # Decompress front sprite data
        front_offset = front_sym.address - start_sym_offset
        front_compressed = rom[front_offset : front_offset + min(front_sym.length, 0x10000)]
        try:
            front_decompressed = bytes(decompress_bytes(front_compressed))
        except Exception:
            continue

        # Decompress back sprite data if available
        back_decompressed = b""
        if back_sym is not None:
            back_offset = back_sym.address - start_sym_offset
            back_compressed = rom[back_offset : back_offset + min(back_sym.length, 0x10000)]
            try:
                back_decompressed = bytes(decompress_bytes(back_compressed))
            except Exception:
                pass

        # Split packed data into individual forms
        num_forms = len(front_decompressed) // FORM_SIZE
        for i, form_name in enumerate(form_names[:num_forms]):
            key = f"{species}_{form_name}"
            start = i * FORM_SIZE
            end = start + FORM_SIZE

            if key not in results:
                results[key] = {
                    "species": species,
                    "form": form_name,
                    "front_tile_data": b"",
                    "back_tile_data": b"",
                    "palette_data": b"",
                    "shiny_palette_data": b"",
                }

            results[key]["front_tile_data"] = front_decompressed[start:end]
            if back_decompressed:
                back_start = i * FORM_SIZE
                back_end = back_start + FORM_SIZE
                if back_end <= len(back_decompressed):
                    results[key]["back_tile_data"] = back_decompressed[back_start:back_end]

    # Extract palettes for form species
    PALETTE_FORM_SIZE = 32  # 16 colors * 2 bytes each

    palette_species = set()
    for key in results:
        species = key.split("_")[0]
        palette_species.add(species)

    for sym in symbols:
        if not sym.name.startswith("gMonPalette_"):
            continue

        species = sym.name[len("gMonPalette_"):]
        if species not in palette_species:
            continue

        data_offset = sym.address - start_sym_offset
        compressed = rom[data_offset : data_offset + min(sym.length, 0x1000)]
        try:
            pal_data = bytes(decompress_bytes(compressed))
        except Exception:
            continue

        # For packed species, split palette per-form.
        # For non-packed species (e.g. Unown), all forms share the same palette.
        is_packed = species in packed_form_species
        num_palettes = len(pal_data) // PALETTE_FORM_SIZE

        for key in results:
            if not (key.startswith(species + "_") or key == species):
                continue

            if is_packed and num_palettes > 1:
                # Determine which form index this key corresponds to
                form = results[key].get("form", "")
                form_names = packed_form_species[species]
                try:
                    form_idx = form_names.index(form)
                except ValueError:
                    form_idx = 0
                start = form_idx * PALETTE_FORM_SIZE
                end = start + PALETTE_FORM_SIZE
                if end <= len(pal_data):
                    results[key]["palette_data"] = pal_data[start:end]
                else:
                    results[key]["palette_data"] = pal_data[:PALETTE_FORM_SIZE]
            else:
                results[key]["palette_data"] = pal_data

    # Extract shiny palettes
    for sym in symbols:
        if not sym.name.startswith("gMonShinyPalette_"):
            continue

        species = sym.name[len("gMonShinyPalette_"):]
        if species not in palette_species:
            continue

        data_offset = sym.address - start_sym_offset
        compressed = rom[data_offset : data_offset + min(sym.length, 0x1000)]
        try:
            pal_data = bytes(decompress_bytes(compressed))
        except Exception:
            continue

        for key in results:
            if key.startswith(species + "_") or key == species:
                results[key]["shiny_palette_data"] = pal_data

    logger.info("Extracted %d alternate form sprites", len(results))
    return results
