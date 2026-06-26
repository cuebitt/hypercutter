"""Command-line interface for extracting metatiles from GBA Pokemon ROMs."""

import argparse
import json
import logging
import shutil
import sys
import tempfile
import urllib.request
from pathlib import Path
from typing import Any

try:
    import orjson

    def _dumps(obj: Any) -> bytes:
        return bytes(orjson.dumps(obj))

except ImportError:

    def _dumps(obj: Any) -> bytes:
        return json.dumps(obj).encode("utf-8")


from tqdm import tqdm

from hypercutter import extract, load_symbols, extract_all_pokemon_sprites
from hypercutter.classes import (
    GAME_CODE_OFFSET,
    SUPPORTED_GAMES,
    get_game_by_name,
    identify_rom,
)
from hypercutter.renderer import TilesetRenderer
from hypercutter.sprite_renderer import PokemonSpriteRenderer, get_species_name


def setup_logging(verbose: bool = False) -> None:
    """Configure logging based on verbosity level."""
    level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(level=level, format="%(levelname)s: %(message)s")


def strip_symbols(sym_data: str, keep_sprites: bool = False) -> str:
    """
    Strip unused symbols, keeping only those needed for extraction.

    Args:
        sym_data: Raw symbol file contents.
        keep_sprites: If True, also keep Pokemon sprite-related symbols.
    """
    needed_exact = {"Start", "gMapLayouts"}
    needed_prefixes = ("gTileset_", "gMetatiles_")

    if keep_sprites:
        # Add sprite-related symbols
        sprite_exact = {
            "gMonFrontPicTable",
            "gMonBackPicTable",
            "gMonPaletteTable",
            "gMonShinyPaletteTable",
            "gMonFrontPicCoords",
            "gMonBackPicCoords",
        }
        sprite_prefixes = ("gMonFrontPic_", "gMonBackPic_")
        needed_exact = needed_exact | sprite_exact
        needed_prefixes = needed_prefixes + sprite_prefixes

    symbols = load_symbols(sym_data)
    filtered = [
        s
        for s in symbols
        if s.name in needed_exact or s.name.startswith(needed_prefixes)
    ]
    filtered.sort(key=lambda s: s.address)

    lines = []
    for s in filtered:
        addr = f"{s.address:08x}"
        length = f"{s.length:08x}"
        lines.append(f"{addr} {s.scope.value} {length} {s.name}")

    return "\n".join(lines)


def prompt_clear_directory(path: Path) -> bool:
    """Prompt user to clear directory and return their response."""
    if not path.exists():
        return True

    existing = list(path.iterdir())
    if not existing:
        return True

    print(f"\nOutput directory '{path}' contains {len(existing)} existing file(s).")
    while True:
        try:
            response = (
                input("Clear output directory before writing? [y/N] ").strip().lower()
            )
        except (KeyboardInterrupt, EOFError):
            print()
            sys.exit(0)
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
    with open(output_path, "wb") as f:
        f.write(_dumps(cleaned))


def export_images(
    metatiles: dict[str, dict[str, Any]],
    output_dir: Path,
    rom_data: bytes | None = None,
    rom_base_address: int = 0x8000000,
    primary_tile_count: int = 0x200,
    exclude_tilesets: list[str] | None = None,
) -> None:
    """Render and save metatiles as PNG images."""
    exclude_tilesets = exclude_tilesets or []
    output_dir.mkdir(parents=True, exist_ok=True)
    for name, data in tqdm(
        metatiles.items(), desc="Rendering tilesets", unit="tileset"
    ):
        if name in exclude_tilesets:
            continue
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


def export_pokemon_sprites(
    sprites: dict[int, dict[str, Any]],
    output_dir: Path,
    as_spritesheet: bool = False,
    columns: int = 8,
) -> None:
    """
    Export Pokemon sprites to PNG files.

    Args:
        sprites: Dictionary from extract_all_pokemon_sprites.
        output_dir: Directory to export sprites.
        as_spritesheet: If True, combine all sprites into a spritesheet.
        columns: Number of columns in spritesheet.
    """
    output_dir.mkdir(parents=True, exist_ok=True)

    if as_spritesheet:
        # Collect all sprites for spritesheet
        front_sprites = []
        back_sprites = []
        species_order = []

        for species_id in tqdm(
            sorted(sprites.keys()), desc="Rendering sprites", unit="pokemon"
        ):
            sprite_data = sprites[species_id]
            species_name = get_species_name(species_id)

            try:
                # Front sprite
                front_renderer = PokemonSpriteRenderer.from_sprite_data(sprite_data)
                if front_renderer.tile_data:
                    front_img = front_renderer.render()
                    front_sprites.append(front_img)
                    species_order.append((species_id, species_name))

                # Back sprite
                back_renderer = PokemonSpriteRenderer.from_back_sprite_data(sprite_data)
                if back_renderer.tile_data:
                    back_img = back_renderer.render()
                    back_sprites.append(back_img)
            except Exception as e:
                logging.error("Failed to render species %d: %s", species_id, e)

        # Save spritesheets
        if front_sprites:
            front_sheet = PokemonSpriteRenderer.render_spritesheet(
                front_sprites, columns
            )
            front_path = output_dir / "front_spritesheet.png"
            front_sheet.save(front_path)
            logging.info("Saved front spritesheet: %s", front_path)

        if back_sprites:
            back_sheet = PokemonSpriteRenderer.render_spritesheet(
                back_sprites, columns
            )
            back_path = output_dir / "back_spritesheet.png"
            back_sheet.save(back_path)
            logging.info("Saved back spritesheet: %s", back_path)

        # Save species list for reference
        species_list_path = output_dir / "species_list.txt"
        with open(species_list_path, "w") as f:
            for idx, (sid, name) in enumerate(species_order):
                f.write(f"{idx + 1}: {sid} - {name}\n")
        logging.info("Saved species list: %s", species_list_path)
    else:
        # Export individual sprites
        for species_id in tqdm(
            sorted(sprites.keys()), desc="Rendering sprites", unit="pokemon"
        ):
            sprite_data = sprites[species_id]
            species_name = get_species_name(species_id)
            species_dir = output_dir / f"{species_id:03d}_{species_name}"
            species_dir.mkdir(parents=True, exist_ok=True)

            try:
                # Front sprite (normal)
                front_renderer = PokemonSpriteRenderer.from_sprite_data(sprite_data)
                if front_renderer.tile_data:
                    front_img = front_renderer.render()
                    front_path = species_dir / "front.png"
                    front_img.save(front_path)

                # Front sprite (shiny)
                front_shiny_renderer = PokemonSpriteRenderer.from_sprite_data(
                    sprite_data, is_shiny=True
                )
                if front_shiny_renderer.tile_data:
                    front_shiny_img = front_shiny_renderer.render()
                    front_shiny_path = species_dir / "front_shiny.png"
                    front_shiny_img.save(front_shiny_path)

                # Back sprite (normal)
                back_renderer = PokemonSpriteRenderer.from_back_sprite_data(sprite_data)
                if back_renderer.tile_data:
                    back_img = back_renderer.render()
                    back_path = species_dir / "back.png"
                    back_img.save(back_path)

                # Back sprite (shiny)
                back_shiny_renderer = PokemonSpriteRenderer.from_back_sprite_data(
                    sprite_data, is_shiny=True
                )
                if back_shiny_renderer.tile_data:
                    back_shiny_img = back_shiny_renderer.render()
                    back_shiny_path = species_dir / "back_shiny.png"
                    back_shiny_img.save(back_shiny_path)

                logging.debug("Exported sprites for %s", species_name)
            except Exception as e:
                logging.error("Failed to export sprites for %s: %s", species_name, e)


def run(
    sym_path: str,
    rom_path: str,
    output_path: str,
    export_dir: str | None = None,
    clear_output: bool = False,
    primary_tile_count: int = 0x200,
    exclude_tilesets: list[str] | None = None,
    dump_sprites: bool = False,
    as_spritesheet: bool = False,
    spritesheet_columns: int = 8,
) -> dict[str, dict[str, Any]]:
    """
    Extract metatiles and optionally sprites from a GBA Pokemon ROM.

    Args:
        sym_path: Path to the .sym file.
        rom_path: Path to the .gba ROM file.
        output_path: Path for the output JSON file.
        export_dir: Directory to export rendered PNGs.
        clear_output: Whether to clear output directories before writing.
        primary_tile_count: Number of tiles in primary tileset.
        exclude_tilesets: List of tileset names to exclude from PNG export.
        dump_sprites: If True, extract Pokemon battle sprites.
        as_spritesheet: If True, output sprites as spritesheets.
        spritesheet_columns: Number of columns in spritesheet.

    Returns:
        The extracted metatiles dictionary.
    """
    with open(rom_path, "rb") as f:
        rom_data = f.read()

    if clear_output:
        clear_directory(Path(output_path).parent)
        if export_dir:
            clear_directory(Path(export_dir))

    metatiles, rom_base_address = extract(sym_path, rom_data)
    save_output(metatiles, Path(output_path))
    logging.info("Metadata written to: %s", output_path)

    if export_dir:
        export_images(
            metatiles,
            Path(export_dir),
            rom_data,
            rom_base_address,
            primary_tile_count,
            exclude_tilesets,
        )
        logging.info("Images exported to: %s", export_dir)

    # Dump Pokemon sprites if requested
    if dump_sprites:
        sprites_dir = Path(export_dir).parent / "pokemon" / "sprites" if export_dir else Path("out/pokemon/sprites")
        try:
            symbols = load_symbols(sym_path)
            sprites = extract_all_pokemon_sprites(
                rom_data, rom_base_address, symbols
            )
            export_pokemon_sprites(
                sprites,
                sprites_dir,
                as_spritesheet=as_spritesheet,
                columns=spritesheet_columns,
            )
            logging.info("Pokemon sprites exported to: %s", sprites_dir)
        except Exception as e:
            logging.error("Failed to dump sprites: %s", e)

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
    parser.add_argument(
        "--dump-sprites",
        action="store_true",
        help="Dump Pokemon battle sprites (front/back)",
    )
    parser.add_argument(
        "--spritesheet",
        action="store_true",
        help="Output sprites as spritesheets instead of individual PNGs",
    )
    parser.add_argument(
        "--spritesheet-columns",
        type=int,
        default=8,
        help="Columns in spritesheet (default: 8)",
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
        cache_dir = Path(tempfile.gettempdir()) / "hypercutter"
        cache_dir.mkdir(exist_ok=True)
        cached_sym_path = cache_dir / sym_path
        stripped_sym_path = cache_dir / f"{sym_path}.stripped"
        if stripped_sym_path.exists():
            logging.info("Using cached symbol file: %s", stripped_sym_path)
            sym_path = str(stripped_sym_path)
        elif cached_sym_path.exists():
            with open(cached_sym_path, "r", encoding="utf-8") as f:
                sym_data = f.read()
            stripped = strip_symbols(sym_data, keep_sprites=args.dump_sprites)
            with open(stripped_sym_path, "w", encoding="utf-8") as f:
                f.write(stripped)
            logging.info("Stripped and cached symbols to %s", stripped_sym_path)
            sym_path = str(stripped_sym_path)
        else:
            urllib.request.urlretrieve(sym_url, cached_sym_path)
            with open(cached_sym_path, "r", encoding="utf-8") as f:
                sym_data = f.read()
            stripped = strip_symbols(sym_data, keep_sprites=args.dump_sprites)
            with open(stripped_sym_path, "w", encoding="utf-8") as f:
                f.write(stripped)
            logging.info("Downloaded and stripped symbols to %s", stripped_sym_path)
            sym_path = str(stripped_sym_path)

    # Default paths if not specified
    output_path = args.output if args.output else "out/metatiles.json"
    export_dir = args.export if args.export else "out/tilesets"
    clear_output = args.yes

    # Get primary tileset tile count from identified game
    primary_tile_count = 0x200  # Default
    exclude_tilesets = []
    if identified:
        primary_tile_count = identified.game.primary_tileset_tile_count
        # FR/LG have a HoennBuilding tileset that should be excluded
        if identified.game.game_code in (b"BPRE", b"BPGE"):
            exclude_tilesets = ["HoennBuilding"]
    elif args.game:
        game = get_game_by_name(args.game)
        if game:
            primary_tile_count = game.primary_tileset_tile_count
            if game.game_code in (b"BPRE", b"BPGE"):
                exclude_tilesets = ["HoennBuilding"]

    # Prompt for clearing if output directory exists and not auto-confirmed
    if not clear_output and not args.clear:
        output_dir = Path(output_path).parent
        try:
            if prompt_clear_directory(output_dir):
                clear_output = True
        except KeyboardInterrupt:
            return

    try:
        run(
            sym_path,
            args.rom,
            output_path,
            export_dir if args.export is not None or args.output is None else None,
            clear_output=clear_output,
            primary_tile_count=primary_tile_count,
            exclude_tilesets=exclude_tilesets,
            dump_sprites=args.dump_sprites,
            as_spritesheet=args.spritesheet,
            spritesheet_columns=args.spritesheet_columns,
        )
    except KeyboardInterrupt:
        pass
    except Exception as e:
        logging.error("%s", e)
        raise


if __name__ == "__main__":
    main()
