"""Command-line interface for extracting metatiles from pokeemerald ROM."""

import argparse
import json
import logging
from pathlib import Path

from hypercutter import extract
from hypercutter.renderer import TilesetRenderer


def setup_logging(verbose: bool = False) -> None:
    """Configure logging based on verbosity level."""
    level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(level=level, format="%(levelname)s: %(message)s")


def save_output(data: dict, output_path: Path) -> None:
    """Save extraction data to a JSON file (excluding raw binary data, keeping lengths)."""
    output_path.parent.mkdir(parents=True, exist_ok=True)

    def strip_raw(obj):
        if isinstance(obj, dict):
            return {k: strip_raw(v) for k, v in obj.items() if not k.endswith("_raw")}
        elif isinstance(obj, list):
            return [strip_raw(i) for i in obj]
        return obj

    cleaned = strip_raw(data)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(cleaned, f, indent=4)


def export_images(metatiles: dict, output_dir: Path) -> None:
    """Render and save metatiles as PNG images."""
    output_dir.mkdir(parents=True, exist_ok=True)
    for name, data in metatiles.items():
        try:
            renderer = TilesetRenderer(data)
            img = renderer.render()
            img_path = output_dir / f"{name}.png"
            img.save(img_path)
            logging.debug("Exported: %s", img_path)
        except Exception as e:
            logging.error("Failed to render %s: %s", name, e)


def run(sym_path: str, rom_path: str, output_path: str, export_dir: str = None) -> dict:
    """
    Extract metatiles from a pokeemerald ROM.

    Args:
        sym_path: Path to the .sym file.
        rom_path: Path to the .gba ROM file.
        output_path: Path for the output JSON file.
        export_dir: Directory to export rendered PNGs.

    Returns:
        The extracted metatiles dictionary.
    """
    metatiles = extract(sym_path, rom_path)
    save_output(metatiles, Path(output_path))
    logging.info("Metadata written to: %s", output_path)

    if export_dir:
        export_images(metatiles, Path(export_dir))
        logging.info("Images exported to: %s", export_dir)

    return metatiles


def main() -> None:
    """Main entry point for the CLI."""
    parser = argparse.ArgumentParser(
        description="Extract metatiles from pokeemerald ROM"
    )
    parser.add_argument("sym", help="Path to .sym file")
    parser.add_argument("rom", help="Path to .gba ROM file")
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

    args = parser.parse_args()
    setup_logging(args.verbose)

    # Default paths if not specified
    output_path = args.output if args.output else "out/metatiles.json"
    export_dir = args.export if args.export else "out/tilesets"

    # Determine what to extract
    extract_json = args.output is not None or args.export is None
    extract_png = args.export is not None or args.output is None

    try:
        metatiles = extract(args.sym, args.rom)

        if extract_json:
            save_output(metatiles, Path(output_path))
            logging.info("Metadata written to: %s", output_path)

        if extract_png:
            export_images(metatiles, Path(export_dir))
            logging.info("Images exported to: %s", export_dir)

    except Exception as e:
        logging.error("%s", e)
        raise e


if __name__ == "__main__":
    main()
