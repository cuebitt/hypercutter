# Web extraction logic for hypercutter.

import zipfile

from hypercutter import extract
from hypercutter.renderer import TilesetRenderer
from js import updateProgressBar  # type: ignore
import os


def extract_metatiles(sym_path: str, rom_path: str) -> dict:
    """Extract metatiles from ROM, returning JSON-serializable data."""
    os.makedirs("/tmp/output", exist_ok=True)

    updateProgressBar(0)

    metatiles, _ = extract(sym_path, rom_path)

    updateProgressBar(30)

    def strip_raw(obj):
        if isinstance(obj, dict):
            return {k: strip_raw(v) for k, v in obj.items() if not k.endswith("_raw")}
        elif isinstance(obj, list):
            return [strip_raw(i) for i in obj]
        return obj

    updateProgressBar(40)
    return strip_raw(metatiles)


def render_images(metatiles: dict, rom_path: str) -> None:
    """Render metatiles as PNG images and create zip archive."""
    os.makedirs("/tmp/output", exist_ok=True)

    updateProgressBar(60)

    with open(rom_path, "rb") as f:
        rom_data = f.read()

    total = len(metatiles)
    for i, (name, data) in enumerate(metatiles.items()):
        try:
            renderer = TilesetRenderer(data, rom_data)
            img = renderer.render()
            img.save(f"/tmp/output/{name}.png")
        except Exception:
            pass

        if i == 0 or i == total - 1 or i % 10 == 0:
            updateProgressBar(60 + int(35 * i / total))

    updateProgressBar(95)

    with zipfile.ZipFile("/tmp/tilesets.zip", "w") as zf:
        for name in __import__("os").listdir("/tmp/output"):
            if name.endswith(".png"):
                zf.write(f"/tmp/output/{name}", name)

    updateProgressBar(100)
