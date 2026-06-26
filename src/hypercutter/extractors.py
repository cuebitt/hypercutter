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
    POKEMON_FORM_SIZE,
    POKEMON_PALETTE_SIZE,
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
        POKEMON_PALETTE_SIZE,
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


def _species_from_suffix(suffix: str, species_names: list[str]) -> tuple[str, str] | None:
    """
    Derive (species, form_id) from a symbol suffix.

    e.g. "UnownB" with species "Unown" → ("unown", "B")
    Falls back to numeric IDs when the form name can't be cleanly extracted.
    """
    suffix_lower = suffix.lower()
    norm_suffix = "".join(c for c in suffix_lower if c.isalnum())

    # Build normalized species name index
    norm_map: dict[str, str] = {}
    for name in sorted(species_names, key=len, reverse=True):
        if not name:
            continue
        norm_name = "".join(c for c in name if c.isalnum())
        # Store the first (longest) match
        if norm_name not in norm_map:
            norm_map[norm_name] = name

    # Check exact normalized match first
    if norm_suffix in norm_map:
        species = norm_map[norm_suffix]
        if suffix_lower == species:
            return None  # exact match → base species
        # Normalized exact match → base species (handles HoOh → ho-oh)
        norm_species = "".join(c for c in species if c.isalnum())
        if norm_suffix == norm_species:
            return None
        # Suffix starts with species name and has more after it → form
        if suffix_lower.startswith(species) and len(suffix_lower) > len(species):
            return (species, suffix[len(species):])  # preserve original case
        # Normalized prefix match for form detection
        if norm_suffix.startswith(norm_species) and len(norm_suffix) > len(norm_species):
            si = 0
            for idx, c in enumerate(suffix_lower):
                if c.isalnum():
                    si += 1
                    if si > len(norm_species):
                        return (species, suffix[idx:])

    # Check prefix matches
    for norm_name, species in sorted(norm_map.items(), key=lambda x: -len(x[0])):
        if norm_suffix.startswith(norm_name) and len(norm_suffix) > len(norm_name):
            si = 0
            for idx, c in enumerate(suffix_lower):
                if c.isalnum():
                    si += 1
                    if si > len(norm_name):
                        return (species, suffix[idx:])  # preserve original case

    return None


def extract_pokemon_form_sprites(
    rom: bytes,
    start_sym_offset: int,
    symbols: list[Offset],
) -> dict[str, dict[str, Any]]:
    """
    Extract alternate form sprites from individual ROM symbols and
    packed data (all derived from ROM, no hardcoded form lists).

    Returns:
        Dictionary mapping "Species_FormId" to sprite data.
    """
    results: dict[str, dict[str, Any]] = {}
    name_index = build_field_index(symbols, "name")

    # Load species names from ROM for form derivation
    species_names = load_species_names(rom, symbols)

    # Build set of data pointers from the main sprite tables (valid species only).
    front_table_sym = name_index.get("gMonFrontPicTable")
    back_table_sym = name_index.get("gMonBackPicTable")
    table_ptrs: set[int] = set()
    num_species = len(species_names)
    if front_table_sym:
        to = front_table_sym.address - start_sym_offset
        for i in range(num_species):
            table_ptrs.add(struct.unpack_from("<I", rom, to + i * 8)[0])
    if back_table_sym:
        to = back_table_sym.address - start_sym_offset
        for i in range(num_species):
            table_ptrs.add(struct.unpack_from("<I", rom, to + i * 8)[0])

    # --- Pass 1: individual form symbols (NOT in the main table) ---
    for sym in symbols:
        if not (sym.name.startswith("gMonFrontPic_") or sym.name.startswith("gMonBackPic_")):
            continue

        # Skip base-species symbols (already handled by the main table extraction)
        if sym.address in table_ptrs:
            continue

        # Derive species + form from symbol name
        suffix = sym.name.split("_", 1)[1] if "_" in sym.name else ""
        parsed = _species_from_suffix(suffix, species_names)
        if parsed is None:
            continue
        species, form_id = parsed
        key = f"{species}_{form_id}"

        if key not in results:
            results[key] = {
                "species": species,
                "form": form_id,
                "front_tile_data": b"",
                "back_tile_data": b"",
                "palette_data": b"",
                "shiny_palette_data": b"",
            }

        data_offset = sym.address - start_sym_offset
        compressed = rom[data_offset : data_offset + min(sym.length, MAX_DECOMPRESS_READ_SIZE)]
        try:
            decompressed = bytes(decompress_bytes(compressed))
        except Exception:
            continue

        if len(decompressed) > POKEMON_FORM_SIZE:
            decompressed = decompressed[:POKEMON_FORM_SIZE]

        if sym.name.startswith("gMonFrontPic_"):
            results[key]["front_tile_data"] = decompressed
        elif sym.name.startswith("gMonBackPic_"):
            results[key]["back_tile_data"] = decompressed

    # --- Pass 2: packed form data ---
    # Detect packed species: both front and back pic must decompress to >= 2 * POKEMON_FORM_SIZE.
    # Regular species may have a front pic that decompresses larger due to LZ77 garbage,
    # but their back pic will only decompress to 1 * POKEMON_FORM_SIZE.
    packed_species: dict[str, list[str]] = {}
    for sym in symbols:
        if not sym.name.startswith("gMonFrontPic_"):
            continue
        suffix = sym.name[len("gMonFrontPic_"):]
        species_lower = suffix.lower()
        if species_lower not in species_names:
            continue

        # Get back pic for this species
        back_sym = name_index.get(f"gMonBackPic_{suffix}")
        if back_sym is None:
            continue

        front_offset = sym.address - start_sym_offset
        back_offset = back_sym.address - start_sym_offset

        try:
            front_data = bytes(decompress_bytes(
                rom[front_offset : front_offset + min(sym.length, MAX_DECOMPRESS_READ_SIZE)]
            ))
            back_data = bytes(decompress_bytes(
                rom[back_offset : back_offset + min(back_sym.length, MAX_DECOMPRESS_READ_SIZE)]
            ))
        except Exception:
            continue

        front_forms = len(front_data) // POKEMON_FORM_SIZE
        back_forms = len(back_data) // POKEMON_FORM_SIZE
        if front_forms < 2 or back_forms < 2:
            continue

        # Both front and back have packed data — use the front form count
        packed_species[species_lower] = [str(i) for i in range(front_forms)]

        for i in range(front_forms):
            key = f"{species_lower}_{i}"
            if key not in results:
                results[key] = {
                    "species": species_lower,
                    "form": str(i),
                    "front_tile_data": b"",
                    "back_tile_data": b"",
                    "palette_data": b"",
                    "shiny_palette_data": b"",
                }
            results[key]["front_tile_data"] = front_data[i * POKEMON_FORM_SIZE : (i + 1) * POKEMON_FORM_SIZE]
            if i * POKEMON_FORM_SIZE + POKEMON_FORM_SIZE <= len(back_data):
                results[key]["back_tile_data"] = back_data[i * POKEMON_FORM_SIZE : (i + 1) * POKEMON_FORM_SIZE]

    # --- Pass 3: palettes ---

    palette_species = set()
    for key in results:
        palette_species.add(results[key]["species"])

    for sym in symbols:
        if not sym.name.startswith("gMonPalette_"):
            continue

        species = sym.name[len("gMonPalette_"):].lower()
        if species not in palette_species:
            continue

        data_offset = sym.address - start_sym_offset
        compressed = rom[data_offset : data_offset + min(sym.length, 0x1000)]
        try:
            pal_data = bytes(decompress_bytes(compressed))
        except Exception:
            continue

        is_packed = species in packed_species
        num_palettes = len(pal_data) // POKEMON_PALETTE_SIZE

        for key in results:
            if results[key]["species"] != species:
                continue

            if is_packed and num_palettes > 1:
                try:
                    form_idx = int(results[key]["form"])
                except (ValueError, KeyError):
                    form_idx = 0
                start_p = form_idx * POKEMON_PALETTE_SIZE
                end_p = start_p + POKEMON_PALETTE_SIZE
                if end_p <= len(pal_data):
                    results[key]["palette_data"] = pal_data[start_p:end_p]
                else:
                    results[key]["palette_data"] = pal_data[:POKEMON_PALETTE_SIZE]
            else:
                results[key]["palette_data"] = pal_data

    # --- Pass 4: shiny palettes ---
    for sym in symbols:
        if not sym.name.startswith("gMonShinyPalette_"):
            continue

        species = sym.name[len("gMonShinyPalette_"):].lower()
        if species not in palette_species:
            continue

        data_offset = sym.address - start_sym_offset
        compressed = rom[data_offset : data_offset + min(sym.length, 0x1000)]
        try:
            pal_data = bytes(decompress_bytes(compressed))
        except Exception:
            continue

        for key in results:
            if results[key]["species"] == species:
                results[key]["shiny_palette_data"] = pal_data

    logger.info("Extracted %d alternate form sprites", len(results))
    return results


# Pokemon text character encoding map (GBA games use custom encoding)
_POKEMON_CHAR_MAP: dict[int, str] = {
    0x00: "", 0xFF: "",
    0xBB: "A", 0xBC: "B", 0xBD: "C", 0xBE: "D", 0xBF: "E",
    0xC0: "F", 0xC1: "G", 0xC2: "H", 0xC3: "I", 0xC4: "J",
    0xC5: "K", 0xC6: "L", 0xC7: "M", 0xC8: "N", 0xC9: "O",
    0xCA: "P", 0xCB: "Q", 0xCC: "R", 0xCD: "S", 0xCE: "T",
    0xCF: "U", 0xD0: "V", 0xD1: "W", 0xD2: "X", 0xD3: "Y",
    0xD4: "Z",
    0xA1: "0", 0xA2: "1", 0xA3: "2", 0xA4: "3", 0xA5: "4",
    0xA6: "5", 0xA7: "6", 0xA8: "7", 0xA9: "8", 0xAA: "9",
    0xAB: "!", 0xAC: "?",
    0xAD: " ",  # MR_MIME separator
    0xAE: "-",  # HO_OH dash
    0xB5: "M",  # male symbol → "m"
    0xB6: "F",  # female symbol → "f"
    0xE0: "'",  # apostrophe (FARFETCH'D)
    0xE1: "D",
}

_POKEMON_NAME_LENGTH = 11


def load_species_names(
    rom: bytes,
    symbols: list[Offset],
) -> list[str]:
    """
    Load Pokemon species names from the ROM.

    Reads the gSpeciesNames table (fixed-length strings using the
    game's custom character encoding) and returns a list of lowercase names.

    Args:
        rom: Raw ROM bytes.
        symbols: List of symbol offsets.

    Returns:
        List of lowercase species names indexed by species ID.
    """
    name_index = build_field_index(symbols, "name")
    sym = name_index.get("gSpeciesNames")
    if sym is None:
        raise ValueError("Symbol 'gSpeciesNames' not found in symbols file")

    start_sym = name_index.get("Start")
    if start_sym is None:
        raise ValueError("Symbol 'Start' not found in symbols file")

    table_offset = sym.address - start_sym.address
    species_count = sym.length // _POKEMON_NAME_LENGTH
    names = []

    for i in range(species_count):
        offset = table_offset + i * _POKEMON_NAME_LENGTH
        raw = rom[offset : offset + _POKEMON_NAME_LENGTH]
        name = ""
        for b in raw:
            ch = _POKEMON_CHAR_MAP.get(b, "")
            if ch:
                name += ch
        names.append(name.lower())

    return names
