"""Pokemon species name loading from GBA ROMs."""

import logging

from ..classes import Offset
from ..utils import build_field_index

logger = logging.getLogger(__name__)

__all__ = ["load_species_names"]

# Pokemon text character encoding map (GBA games use custom encoding)
_POKEMON_CHAR_MAP: dict[int, str] = {
    0x00: "",
    0xFF: "",
    0xBB: "A",
    0xBC: "B",
    0xBD: "C",
    0xBE: "D",
    0xBF: "E",
    0xC0: "F",
    0xC1: "G",
    0xC2: "H",
    0xC3: "I",
    0xC4: "J",
    0xC5: "K",
    0xC6: "L",
    0xC7: "M",
    0xC8: "N",
    0xC9: "O",
    0xCA: "P",
    0xCB: "Q",
    0xCC: "R",
    0xCD: "S",
    0xCE: "T",
    0xCF: "U",
    0xD0: "V",
    0xD1: "W",
    0xD2: "X",
    0xD3: "Y",
    0xD4: "Z",
    0xA1: "0",
    0xA2: "1",
    0xA3: "2",
    0xA4: "3",
    0xA5: "4",
    0xA6: "5",
    0xA7: "6",
    0xA8: "7",
    0xA9: "8",
    0xAA: "9",
    0xAB: "!",
    0xAC: "?",
    0xAD: " ",  # MR_MIME separator
    0xAE: "-",  # HO_OH dash
    0xB5: "M",  # male symbol -> "m"
    0xB6: "F",  # female symbol -> "f"
    0xE0: "'",  # apostrophe (FARFETCH'D)
    0xE1: "D",
}

_POKEMON_NAME_LENGTH = 11


def load_species_names(
    rom: bytes,
    symbols: list[Offset],
) -> list[str]:
    """
    Load Pokemon species names from the ROM.

    Reads the gSpeciesNames table (fixed-length strings using the
    game's custom character encoding) and returns a list of lowercase names.

    Args:
        rom: Raw ROM bytes.
        symbols: List of symbol offsets.

    Returns:
        List of lowercase species names indexed by species ID.
    """
    name_index = build_field_index(symbols, "name")
    sym = name_index.get("gSpeciesNames")
    if sym is None:
        raise ValueError("Symbol 'gSpeciesNames' not found in symbols file")

    start_sym = name_index.get("Start")
    if start_sym is None:
        raise ValueError("Symbol 'Start' not found in symbols file")

    table_offset = sym.address - start_sym.address
    species_count = sym.length // _POKEMON_NAME_LENGTH
    names = []

    for i in range(species_count):
        offset = table_offset + i * _POKEMON_NAME_LENGTH
        raw = rom[offset : offset + _POKEMON_NAME_LENGTH]
        name = ""
        for b in raw:
            ch = _POKEMON_CHAR_MAP.get(b, "")
            if ch:
                name += ch
        names.append(name.lower())

    return names
