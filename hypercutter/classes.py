from dataclasses import dataclass
from enum import Enum


class OffsetType(Enum):
    """Symbol scope: GLOBAL for global symbols, LOCAL for local symbols."""

    GLOBAL = 0
    LOCAL = 1


@dataclass
class Offset:
    """Represents a symbol entry from a .sym file."""

    address: int
    type: OffsetType
    length: int
    name: str


@dataclass
class MapLayout:
    """Represents a map layout structure from the ROM."""

    width: int
    height: int
    border_ptr: int
    map_ptr: int
    primary_tileset_ptr: int
    secondary_tileset_ptr: int


@dataclass
class Tileset:
    """Represents a tileset structure from the ROM."""

    is_compressed: bool
    is_secondary: bool
    tiles_ptr: int
    palettes_ptr: int
    metatiles_ptr: int
    metatile_attributes_ptr: int
    callback_ptr: int
