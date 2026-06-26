MAP_LAYOUT_SIZE = 0x18
MAP_LAYOUT_FORMAT = "<iiIIII"

TILESET_SIZE = 0x18
TILESET_FORMAT = "<??xxIIIII"

DEFAULT_ROM_BASE_ADDRESS = 0x8000000
PALETTE_SIZE = 512
PRIMARY_TILESET_TILE_COUNT = 0x200
TILE_SIZE = 32  # 8x8 pixels * 4 bits per pixel / 8
METATILE_SIZE = 16  # 8 tiles * 2 bytes per metatile entry
METATILE_TILE_COUNT = 8

MAX_DECOMPRESS_READ_SIZE = 0x10000  # Maximum bytes to read for decompression

# Pokemon sprite table sizes
SPRITE_SHEET_ENTRY_SIZE = 8  # CompressedSpriteSheet struct
SPRITE_PALETTE_ENTRY_SIZE = 8  # CompressedSpritePalette struct
MON_COORDS_ENTRY_SIZE = 4  # MonCoords struct (padded to 4 bytes in ROM)
MON_PIC_WIDTH = 64  # All Pokemon pics are stored in 64x64 frames
MON_PIC_HEIGHT = 64

# Symbol names used to locate sprite data tables in the ROM
SPRITE_SYMBOL_NAMES = [
    "gMonFrontPicTable",
    "gMonBackPicTable",
    "gMonPaletteTable",
    "gMonShinyPaletteTable",
    "gMonFrontPicCoords",
    "gMonBackPicCoords",
]
