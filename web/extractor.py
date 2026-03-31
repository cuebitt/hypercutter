# Web extraction logic for hypercutter.

import json
import zipfile

from hypercutter import extract
from hypercutter.classes import GAME_CODE_OFFSET, SUPPORTED_GAMES
from hypercutter.renderer import TilesetRenderer
from js import updateProgressBar  # type: ignore
import os


def get_primary_tile_count(rom_path: str) -> int:
    """Get the primary tileset tile count based on game code."""
    with open(rom_path, "rb") as f:
        rom_data = f.read()
    game_code = rom_data[GAME_CODE_OFFSET : GAME_CODE_OFFSET + 4]
    game = SUPPORTED_GAMES.get(game_code)
    if game:
        return game.primary_tileset_tile_count
    return 0x200  # Default


def get_exclude_tilesets(rom_path: str) -> list[str]:
    """Get tilesets to exclude based on game code."""
    with open(rom_path, "rb") as f:
        rom_data = f.read()
    game_code = rom_data[GAME_CODE_OFFSET : GAME_CODE_OFFSET + 4]
    # FR/LG have a HoennBuilding tileset that should be excluded
    if game_code in (b"BPRE", b"BPGE"):
        return ["HoennBuilding"]
    return []


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

    stripped = strip_raw(metatiles)

    with open("/tmp/output/metatiles.json", "w") as f:
        json.dump(stripped, f)

    updateProgressBar(40)
    return stripped


def render_images(metatiles: dict, rom_path: str) -> None:
    """Render metatiles as PNG images and create zip archive."""
    os.makedirs("/tmp/output", exist_ok=True)

    updateProgressBar(60)

    with open(rom_path, "rb") as f:
        rom_data = f.read()

    primary_tile_count = get_primary_tile_count(rom_path)
    exclude_tilesets = get_exclude_tilesets(rom_path)

    total = len(metatiles)
    for i, (name, data) in enumerate(metatiles.items()):
        if name in exclude_tilesets:
            continue
        try:
            renderer = TilesetRenderer(
                data, rom_data, primary_tile_count=primary_tile_count
            )
            img = renderer.render()
            img.save(f"/tmp/output/{name}.png")
        except Exception:
            pass

        if i == 0 or i == total - 1 or i % 10 == 0:
            updateProgressBar(60 + int(35 * i / total))

    updateProgressBar(95)

    with zipfile.ZipFile("/tmp/tilesets.zip", "w") as zf:
        for name in __import__("os").listdir("/tmp/output"):
            zf.write(f"/tmp/output/{name}", name)

    updateProgressBar(100)
