"""Shared GBA graphics primitives for palette and tile rendering."""

from PIL import Image

from .utils import decode_bgr555

__all__ = [
    "decode_palettes",
    "render_tile_to_image",
    "render_pixels_to_image",
]


def decode_palettes(
    raw_data: bytes, num_palettes: int = 16, colors_per_palette: int = 16
) -> list[list[tuple[int, int, int]]]:
    """Decode BGR555 palette data into a list of palettes, each with RGB tuples."""
    palettes: list[list[tuple[int, int, int]]] = []
    for i in range(num_palettes):
        palette: list[tuple[int, int, int]] = []
        for j in range(colors_per_palette):
            offset = (i * colors_per_palette + j) * 2
            if offset + 2 <= len(raw_data):
                color_val = int.from_bytes(raw_data[offset : offset + 2], "little")
                palette.append(decode_bgr555(color_val))
            else:
                palette.append((0, 0, 0))
        palettes.append(palette)
    return palettes


def render_tile_to_image(
    tile_data: bytes,
    palette: list[tuple[int, int, int]],
    h_flip: bool = False,
    v_flip: bool = False,
    is_transparent: bool = False,
) -> Image.Image:
    """Render a single 8x8 4bpp tile as a PIL Image."""
    from .utils import decode_tile_4bpp

    indices = decode_tile_4bpp(tile_data)
    pixels = bytearray(64 * 4)  # 8x8 RGBA

    for i, idx in enumerate(indices):
        offset = i * 4
        if is_transparent and idx == 0:
            pixels[offset] = 0
            pixels[offset + 1] = 0
            pixels[offset + 2] = 0
            pixels[offset + 3] = 0
        else:
            r, g, b = palette[idx % len(palette)]
            pixels[offset] = r
            pixels[offset + 1] = g
            pixels[offset + 2] = b
            pixels[offset + 3] = 255

    img = Image.frombytes("RGBA", (8, 8), bytes(pixels))

    if h_flip:
        img = img.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
    if v_flip:
        img = img.transpose(Image.Transpose.FLIP_TOP_BOTTOM)

    return img


def render_pixels_to_image(
    pixel_indices: list[list[int]],
    palette: list[tuple[int, int, int]],
    width: int,
    height: int,
    is_transparent: bool = True,
) -> Image.Image:
    """Render a 2D grid of palette indices as a PIL Image."""
    pixels = bytearray(width * height * 4)

    for y_idx in range(height):
        for x_idx in range(width):
            idx = pixel_indices[y_idx][x_idx]
            offset = (y_idx * width + x_idx) * 4

            if is_transparent and idx == 0:
                pixels[offset] = 0
                pixels[offset + 1] = 0
                pixels[offset + 2] = 0
                pixels[offset + 3] = 0
            else:
                r, g, b = palette[idx % len(palette)] if palette else (0, 0, 0)
                pixels[offset] = r
                pixels[offset + 1] = g
                pixels[offset + 2] = b
                pixels[offset + 3] = 255

    return Image.frombytes("RGBA", (width, height), bytes(pixels))
