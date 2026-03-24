"""Command-line interface for extracting metatiles from pokeemerald ROM."""

import argparse
import json
import logging
import sys
from pathlib import Path

from hypercutter import extract


def setup_logging(verbose: bool = False) -> None:
    """Configure logging based on verbosity level."""
    level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(level=level, format="%(levelname)s: %(message)s")


def save_output(data: dict, output_path: Path) -> None:
    """Save extraction data to a JSON file."""
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=4)


def run(sym_path: str, rom_path: str, output_path: str) -> dict:
    """
    Extract metatiles from a pokeemerald ROM.

    Args:
        sym_path: Path to the .sym file.
        rom_path: Path to the .gba ROM file.
        output_path: Path for the output JSON file.

    Returns:
        The extracted metatiles dictionary.
    """
    metatiles = extract(sym_path, rom_path)
    save_output(metatiles, Path(output_path))
    logging.info("Output written to: %s", output_path)
    return metatiles


def main() -> None:
    """Main entry point for the CLI."""
    parser = argparse.ArgumentParser(
        description="Extract metatiles from pokeemerald ROM"
    )
    parser.add_argument("sym", help="Path to .sym file")
    parser.add_argument("rom", help="Path to .gba ROM file")
    parser.add_argument(
        "-o", "--output", default="out/metatiles.json", help="Output file path"
    )
    parser.add_argument(
        "-v", "--verbose", action="store_true", help="Enable verbose output"
    )

    args = parser.parse_args()
    setup_logging(args.verbose)

    try:
        run(args.sym, args.rom, args.output)
    except Exception as e:
        logging.error("%s", e)
        sys.exit(1)


if __name__ == "__main__":
    main()
