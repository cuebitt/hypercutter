from typing import Any, TypeVar

T = TypeVar("T")


def find_by_field(items: list[T], field: str, value: Any) -> T | None:
    """
    Find the first item in a list that has an attribute matching the given value.

    Args:
        items: List of objects to search.
        field: Name of the attribute to check.
        value: Value to match against.

    Returns:
        The first matching item, or None if not found.
    """
    return next((item for item in items if getattr(item, field) == value), None)


def find_primary_from_secondary(
    pairs: dict[str, list[str]], secondary: str
) -> str | None:
    """
    Find the primary tileset name for a given secondary tileset.

    Args:
        pairs: Dictionary mapping primary tileset names to lists of secondary tileset names.
        secondary: The secondary tileset name to look up.

    Returns:
        The primary tileset name if found, or None.
    """
    if secondary in pairs:
        return secondary
    for primary, secondary_list in pairs.items():
        if secondary in secondary_list:
            return primary
    return None


def decode_bgr555(color16: int) -> tuple[int, int, int]:
    """Convert 16-bit BGR555 to RGB888."""
    r = color16 & 0x1F
    g = (color16 >> 5) & 0x1F
    b = (color16 >> 10) & 0x1F

    # Scale to 0-255
    r = (r << 3) | (r >> 2)
    g = (g << 3) | (g >> 2)
    b = (b << 3) | (b >> 2)

    return (r, g, b)


def decode_tile_4bpp(data: bytes) -> list[int]:
    """Decode 32 bytes of 4bpp data into 64 pixel indices."""
    pixels = []
    for byte in data:
        pixels.append(byte & 0x0F)
        pixels.append(byte >> 4)
    return pixels


def parse_metatile_entry(entry: int) -> tuple[int, bool, bool, int]:
    """
    Parse a 16-bit metatile entry.

    Layout:
    - bits 0-9: tile index
    - bit 10: horizontal flip
    - bit 11: vertical flip
    - bits 12-15: palette index

    Returns:
        (tile_index, h_flip, v_flip, palette_index)
    """
    tile_index = entry & 0x3FF
    h_flip = bool((entry >> 10) & 1)
    v_flip = bool((entry >> 11) & 1)
    palette_index = (entry >> 12) & 0xF
    return tile_index, h_flip, v_flip, palette_index
