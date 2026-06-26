"""TypedDict definitions for hypercutter extracted data structures."""

from typing import TypedDict

from .classes import MonCoords, SpritePalette, SpriteSheet


class TilesetData(TypedDict, total=False):
    """Data extracted from a single tileset."""

    is_compressed: bool
    is_secondary: bool
    tiles_ptr: int
    palettes_ptr: int
    metatiles_ptr: int
    metatile_attributes_ptr: int
    callback_ptr: int
    tiles_raw: bytes
    tiles_length: int
    palettes_raw: bytes
    palettes_length: int
    metatiles_raw: bytes
    metatiles_length: int


class MetatileEntry(TypedDict):
    """A metatile entry with primary and optional secondary tileset."""

    primary: TilesetData
    secondary: TilesetData | None


class SpriteEntry(TypedDict):
    """Extracted sprite data for a single Pokemon species."""

    front: SpriteSheet
    back: SpriteSheet
    palette: SpritePalette
    shiny_palette: SpritePalette
    front_coords: MonCoords
    back_coords: MonCoords
    front_tile_data: bytes
    back_tile_data: bytes
    palette_data: bytes
    shiny_palette_data: bytes


class FormSpriteEntry(TypedDict, total=False):
    """Extracted sprite data for an alternate form."""

    species: str
    form: str
    front_tile_data: bytes
    back_tile_data: bytes
    palette_data: bytes
    shiny_palette_data: bytes


# Composite result type for the extract() orchestrator
ExtractResult = tuple[dict[str, MetatileEntry], int]
