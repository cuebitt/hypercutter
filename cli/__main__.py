"""Command-line interface for extracting metatiles from GBA Pokemon ROMs."""

import argparse
import json
import logging
import shutil
import urllib.request
from pathlib import Path
from typing import Any

from tqdm import tqdm

from hypercutter import extract
from hypercutter.classes import (
    GAME_CODE_OFFSET,
    SUPPORTED_GAMES,
    get_game_by_name,
    identify_rom,
)
from hypercutter.renderer import TilesetRenderer


def setup_logging(verbose: bool = False) -> None:
    """Configure logging based on verbosity level."""
    level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(level=level, format="%(levelname)s: %(message)s")


def prompt_clear_directory(path: Path) -> bool:
    """Prompt user to clear directory and return their response."""
    if not path.exists():
        return True

    existing = list(path.iterdir())
    if not existing:
        return True

    print(f"\nOutput directory '{path}' contains {len(existing)} existing file(s).")
    while True:
        response = (
            input("Clear output directory before writing? [y/N] ").strip().lower()
        )
        if response in ("y", "yes"):
            return True
        elif response in ("", "n", "no"):
            return False
        else:
            print("Please enter 'y' or 'n'")


def clear_directory(path: Path) -> None:
    """Remove all contents of a directory."""
    if path.exists():
        for item in path.iterdir():
            if item.is_file():
                item.unlink()
            elif item.is_dir():
                shutil.rmtree(item)


def strip_raw(obj: dict | list) -> dict | list:
    """Remove raw binary data from extracted tileset data."""
    if isinstance(obj, dict):
        return {k: strip_raw(v) for k, v in obj.items() if not k.endswith("_raw")}
    elif isinstance(obj, list):
        return [strip_raw(i) for i in obj]
    return obj


def save_output(data: dict, output_path: Path) -> None:
    """Save extraction data to a JSON file (excluding raw binary data, keeping lengths)."""
    output_path.parent.mkdir(parents=True, exist_ok=True)
    cleaned = strip_raw(data)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(cleaned, f, indent=4)


def export_images(
    metatiles: dict[str, dict[str, Any]],
    output_dir: Path,
    rom_data: bytes | None = None,
    rom_base_address: int = 0x8000000,
    primary_tile_count: int = 0x200,
) -> None:
    """Render and save metatiles as PNG images."""
    output_dir.mkdir(parents=True, exist_ok=True)
    for name, data in tqdm(
        metatiles.items(), desc="Rendering tilesets", unit="tileset"
    ):
        try:
            renderer = TilesetRenderer(
                data, rom_data, rom_base_address, primary_tile_count
            )
            img = renderer.render()
            img_path = output_dir / f"{name}.png"
            img.save(img_path)
            logging.debug("Exported: %s", img_path)
        except Exception as e:
            logging.error("Failed to render %s: %s", name, e)


def run(
    sym_path: str,
    rom_path: str,
    output_path: str,
    export_dir: str | None = None,
    clear_output: bool = False,
    primary_tile_count: int = 0x200,
) -> dict[str, dict[str, Any]]:
    """
    Extract metatiles from a GBA Pokemon ROM.

    Args:
        sym_path: Path to the .sym file.
        rom_path: Path to the .gba ROM file.
        output_path: Path for the output JSON file.
        export_dir: Directory to export rendered PNGs.
        clear_output: Whether to clear output directories before writing.
        primary_tile_count: Number of tiles in primary tileset.

    Returns:
        The extracted metatiles dictionary.
    """
    rom_data = None
    if export_dir:
        with open(rom_path, "rb") as f:
            rom_data = f.read()

    if clear_output:
        clear_directory(Path(output_path).parent)
        if export_dir:
            clear_directory(Path(export_dir))

    metatiles, rom_base_address = extract(sym_path, rom_path)
    save_output(metatiles, Path(output_path))
    logging.info("Metadata written to: %s", output_path)

    if export_dir:
        export_images(
            metatiles, Path(export_dir), rom_data, rom_base_address, primary_tile_count
        )
        logging.info("Images exported to: %s", export_dir)

    return metatiles


def main() -> None:
    """Main entry point for the CLI."""
    game_names = ", ".join(g.name for g in SUPPORTED_GAMES.values())
    parser = argparse.ArgumentParser(
        description="Extract metatiles from GBA Pokemon ROMs"
    )
    parser.add_argument(
        "sym",
        nargs="?",
        help="Path to .sym file (will download if not provided)",
    )
    parser.add_argument("rom", help="Path to .gba ROM file")
    parser.add_argument(
        "-g",
        "--game",
        choices=[g.short_name for g in SUPPORTED_GAMES.values()],
        help=f"Game to use for symbol file download. Supported: {game_names}",
    )
    parser.add_argument(
        "-o",
        "--output",
        metavar="PATH",
        help="Output JSON path (default: out/metatiles.json)",
    )
    parser.add_argument(
        "-e",
        "--export",
        metavar="DIR",
        help="Directory to export PNGs (default: out/tilesets)",
    )
    parser.add_argument(
        "-v", "--verbose", action="store_true", help="Enable verbose output"
    )
    parser.add_argument(
        "-c",
        "--clear",
        action="store_true",
        help="Clear output directory before writing",
    )
    parser.add_argument(
        "-y",
        "--yes",
        action="store_true",
        help="Automatically clear output directory without prompting",
    )

    args = parser.parse_args()
    setup_logging(args.verbose)

    # Read ROM data for game/sym detection
    with open(args.rom, "rb") as f:
        rom_data = f.read()

    # Identify ROM by hash
    identified = identify_rom(rom_data)

    # Download sym file if not provided
    sym_path = args.sym
    if not sym_path:
        if identified:
            sym_path = identified.sym_filename
            sym_url = identified.sym_url
            logging.info(
                "Identified ROM: %s (%s)", identified.game.name, identified.sym_filename
            )
        elif args.game:
            # Fallback: use header game code or explicit --game
            game = get_game_by_name(args.game)
            if game is None:
                logging.error("Unknown game: %s", args.game)
                return
            sym_path = game.default_sym_filename
            sym_url = game.sym_url
            logging.info("Using game from --game flag: %s", game.name)
        else:
            # No hash match and no --game specified
            game_code = (
                rom_data[GAME_CODE_OFFSET : GAME_CODE_OFFSET + 4]
                if len(rom_data) >= GAME_CODE_OFFSET + 4
                else b"????"
            )
            logging.error(
                "Unidentified ROM. Game code: %s. "
                "If you believe this is a supported ROM, please report this issue. "
                "You can also try specifying --game explicitly or providing a .sym file.",
                game_code.decode("latin-1", errors="replace"),
            )
            return
        logging.info("Downloading symbol file from %s", sym_url)
        urllib.request.urlretrieve(sym_url, sym_path)
        logging.info("Downloaded to %s", sym_path)

    # Default paths if not specified
    output_path = args.output if args.output else "out/metatiles.json"
    export_dir = args.export if args.export else "out/tilesets"
    clear_output = args.yes

    # Get primary tileset tile count from identified game
    primary_tile_count = 0x200  # Default
    if identified:
        primary_tile_count = identified.game.primary_tileset_tile_count
    elif args.game:
        game = get_game_by_name(args.game)
        if game:
            primary_tile_count = game.primary_tileset_tile_count

    # Prompt for clearing if output directory exists and not auto-confirmed
    if not clear_output and not args.clear:
        output_dir = Path(output_path).parent
        if prompt_clear_directory(output_dir):
            clear_output = True

    try:
        run(
            sym_path,
            args.rom,
            output_path,
            export_dir if args.export is not None or args.output is None else None,
            clear_output=clear_output,
            primary_tile_count=primary_tile_count,
        )
    except Exception as e:
        logging.error("%s", e)
        raise e


if __name__ == "__main__":
    main()
