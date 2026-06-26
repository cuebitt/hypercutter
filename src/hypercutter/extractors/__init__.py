"""Extraction functions for GBA Pokemon ROM data.

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

from .species import load_species_names
from .sprites import (
    extract_all_pokemon_sprites,
    extract_mon_coords,
    extract_palette_data,
    extract_pokemon_form_sprites,
    extract_sprite_data,
    extract_sprite_palette,
    extract_sprite_sheet,
    extract_sprite_table,
)
from .symbols import load_symbols, read_rom, validate_rom
from .tilesets import (
    build_tileset_name_pairs,
    extract,
    extract_all_tilesets,
    extract_map_layout,
    extract_map_table,
    extract_metatile_info,
    extract_metatiles,
    extract_raw_data,
    extract_tileset,
    extract_tileset_info,
    extract_tileset_with_raw,
)

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
    "extract_pokemon_form_sprites",
    "load_species_names",
]
