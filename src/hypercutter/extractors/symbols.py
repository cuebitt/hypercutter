"""Symbol loading and ROM validation."""

import logging
from pathlib import Path

from ..classes import (
    GAME_CODE_LENGTH,
    GAME_CODE_OFFSET,
    SUPPORTED_GAMES,
    Offset,
    OffsetType,
)

logger = logging.getLogger(__name__)

__all__ = [
    "validate_rom",
    "load_symbols",
    "read_rom",
]


def validate_rom(rom_data: bytes, expected_game_code: bytes | None = None) -> bool:
    """
    Validate that the ROM has a supported game code.

    Args:
        rom_data: Raw ROM bytes.
        expected_game_code: Optional specific game code to validate against.

    Returns:
        True if validation passed (even with warnings).
    """
    if len(rom_data) < GAME_CODE_OFFSET + GAME_CODE_LENGTH:
        logger.warning("ROM file too small to validate")
        return True
    game_code = rom_data[GAME_CODE_OFFSET : GAME_CODE_OFFSET + GAME_CODE_LENGTH]
    if expected_game_code is not None and game_code != expected_game_code:
        logger.warning(
            "ROM game code '%s' does not match expected '%s'",
            game_code.decode("latin-1"),
            expected_game_code.decode(),
        )
    elif game_code not in SUPPORTED_GAMES:
        logger.warning(
            f"ROM game code '{game_code.decode('latin-1')}' is not a supported game"
        )
    return True


def _parse_symbols(data: str) -> list[Offset]:
    """
    Parse symbol data into Offset objects.

    Symbol file format (one symbol per line):
    08abcde0 l 00000010 gSymbolName
    [address] [type] [length] [name]

    If length is 0 or missing, calculate it using the next symbol's address.
    For gTilesetTiles_* symbols, prefer using the matching gTilesetPalettes_* symbol.

    Args:
        data: Raw symbol file contents.

    Returns:
        List of Offset objects.
    """
    lines = [x.strip().split(" ") for x in data.splitlines() if x.strip()]
    offsets = []

    # Build a map of symbol names to addresses for quick lookup
    name_to_addr = {}
    for line in lines:
        if len(line) >= 4 and line[0] and line[3]:
            try:
                addr = int(f"0x{line[0]}", 0)
                name_to_addr[line[3]] = addr
            except ValueError:
                pass

    # Sort lines by address for correct length calculation
    sorted_lines = sorted(
        (line for line in lines if len(line) >= 4 and line[0]),
        key=lambda x: int(f"0x{x[0]}", 0),
    )

    for i, line in enumerate(sorted_lines):
        address = int(f"0x{line[0]}", 0)
        sym_type = OffsetType.GLOBAL if line[1] == "g" else OffsetType.LOCAL
        length = int(f"0x{line[2]}", 0)
        name = line[3]

        # If still 0, use next symbol
        if length == 0 and i + 1 < len(sorted_lines):
            next_line = sorted_lines[i + 1]
            if next_line[0]:
                next_address = int(f"0x{next_line[0]}", 0)
                if next_address > address:
                    length = next_address - address

        offsets.append(
            Offset(
                address=address,
                scope=sym_type,
                length=length,
                name=name,
            )
        )

    return offsets


def load_symbols(filepath_or_data: str | bytes) -> list[Offset]:
    """Load symbols from a .sym file or raw data."""
    logger.debug("load_symbols: %r", filepath_or_data)
    if (
        isinstance(filepath_or_data, str)
        and "\n" not in filepath_or_data
        and Path(filepath_or_data).is_file()
    ):
        with open(filepath_or_data, "r", encoding="utf-8") as f:
            filepath_or_data = f.read()
    elif isinstance(filepath_or_data, bytes):
        filepath_or_data = filepath_or_data.decode("utf-8")
    return _parse_symbols(filepath_or_data)


def read_rom(filepath_or_data: str | bytes) -> bytes:
    """Read a ROM file into memory."""
    if isinstance(filepath_or_data, str):
        with open(filepath_or_data, "rb") as f:
            return f.read()
    return filepath_or_data
