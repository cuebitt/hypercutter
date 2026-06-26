"""Sprite extraction from GBA Pokemon ROMs."""

import logging
import struct

from ..classes import MonCoords, Offset, SpritePalette, SpriteSheet
from ..constants import (
    MAX_DECOMPRESS_READ_SIZE,
    MON_COORDS_ENTRY_SIZE,
    POKEMON_FORM_SIZE,
    POKEMON_PALETTE_SIZE,
    SPRITE_PALETTE_ENTRY_SIZE,
    SPRITE_SHEET_ENTRY_SIZE,
    SPRITE_SYMBOL_NAMES,
)
from ..types import FormSpriteEntry, SpriteEntry
from ..exceptions import DecompressionError
from ..lzss3 import decompress_bytes
from ..utils import build_field_index

from .tilesets import extract_raw_data
from .species import load_species_names

logger = logging.getLogger(__name__)

__all__ = [
    "extract_sprite_sheet",
    "extract_sprite_palette",
    "extract_mon_coords",
    "extract_sprite_table",
    "extract_sprite_data",
    "extract_palette_data",
    "extract_all_pokemon_sprites",
    "extract_pokemon_form_sprites",
]


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
        data = data[: sprite.size]
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
    species_names: list[str] | None = None,
) -> dict[int, SpriteEntry]:
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

    results: dict[int, SpriteEntry] = {}

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

            # Skip old Unown beta placeholders (detected by name)
            if species_names is not None and 0 <= species_id < len(species_names):
                if species_names[species_id].startswith("old_unown"):
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


def _species_from_suffix(
    suffix: str,
    species_names: list[str],
    norm_map: dict[str, str] | None = None,
) -> tuple[str, str] | None:
    """
    Derive (species, form_id) from a symbol suffix.

    e.g. "UnownB" with species "Unown" -> ("unown", "B")
    Falls back to numeric IDs when the form name can't be cleanly extracted.
    """
    suffix_lower = suffix.lower()
    norm_suffix = "".join(c for c in suffix_lower if c.isalnum())

    # Build normalized species name index (cached via norm_map param)
    if norm_map is None:
        norm_map = {}
        for name in sorted(species_names, key=len, reverse=True):
            if not name:
                continue
            norm_name = "".join(c for c in name if c.isalnum())
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
            return (species, suffix[len(species) :])  # preserve original case
        # Normalized prefix match for form detection
        if norm_suffix.startswith(norm_species) and len(norm_suffix) > len(
            norm_species
        ):
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
) -> dict[str, FormSpriteEntry]:
    """
    Extract alternate form sprites from individual ROM symbols and
    packed data (all derived from ROM, no hardcoded form lists).

    Returns:
        Dictionary mapping "Species_FormId" to sprite data.
    """
    results: dict[str, FormSpriteEntry] = {}
    name_index = build_field_index(symbols, "name")

    # Load species names from ROM for form derivation
    species_names = load_species_names(rom, symbols)

    # Build normalized lookup once for _species_from_suffix
    norm_map: dict[str, str] = {}
    for name in sorted(species_names, key=len, reverse=True):
        if not name:
            continue
        norm_name = "".join(c for c in name if c.isalnum())
        if norm_name not in norm_map:
            norm_map[norm_name] = name

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
        if not (
            sym.name.startswith("gMonFrontPic_") or sym.name.startswith("gMonBackPic_")
        ):
            continue

        # Skip base-species symbols (already handled by the main table extraction)
        if sym.address in table_ptrs:
            continue

        # Derive species + form from symbol name
        suffix = sym.name.split("_", 1)[1] if "_" in sym.name else ""
        parsed = _species_from_suffix(suffix, species_names, norm_map)
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
        compressed = rom[
            data_offset : data_offset + min(sym.length, MAX_DECOMPRESS_READ_SIZE)
        ]
        try:
            decompressed = bytes(decompress_bytes(compressed))
        except DecompressionError:
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
        suffix = sym.name[len("gMonFrontPic_") :]
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
            front_data = bytes(
                decompress_bytes(
                    rom[
                        front_offset : front_offset
                        + min(sym.length, MAX_DECOMPRESS_READ_SIZE)
                    ]
                )
            )
            back_data = bytes(
                decompress_bytes(
                    rom[
                        back_offset : back_offset
                        + min(back_sym.length, MAX_DECOMPRESS_READ_SIZE)
                    ]
                )
            )
        except DecompressionError:
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
            results[key]["front_tile_data"] = front_data[
                i * POKEMON_FORM_SIZE : (i + 1) * POKEMON_FORM_SIZE
            ]
            if i * POKEMON_FORM_SIZE + POKEMON_FORM_SIZE <= len(back_data):
                results[key]["back_tile_data"] = back_data[
                    i * POKEMON_FORM_SIZE : (i + 1) * POKEMON_FORM_SIZE
                ]

    # --- Pass 3: palettes ---

    palette_species = set()
    for key in results:
        palette_species.add(results[key]["species"])

    for sym in symbols:
        if not sym.name.startswith("gMonPalette_"):
            continue

        species = sym.name[len("gMonPalette_") :].lower()
        if species not in palette_species:
            continue

        data_offset = sym.address - start_sym_offset
        compressed = rom[data_offset : data_offset + min(sym.length, 0x1000)]
        try:
            pal_data = bytes(decompress_bytes(compressed))
        except DecompressionError:
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

        species = sym.name[len("gMonShinyPalette_") :].lower()
        if species not in palette_species:
            continue

        data_offset = sym.address - start_sym_offset
        compressed = rom[data_offset : data_offset + min(sym.length, 0x1000)]
        try:
            pal_data = bytes(decompress_bytes(compressed))
        except DecompressionError:
            continue

        for key in results:
            if results[key]["species"] == species:
                results[key]["shiny_palette_data"] = pal_data

    logger.info("Extracted %d alternate form sprites", len(results))
    return results
