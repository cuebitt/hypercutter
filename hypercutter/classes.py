from dataclasses import dataclass
from enum import Enum
from hashlib import sha256


GAME_CODE_OFFSET = 0xAC
GAME_CODE_LENGTH = 4


@dataclass
class GameProfile:
    """Configuration for a specific GBA Pokemon ROM."""

    game_code: bytes
    name: str
    short_name: str
    sym_repo: str
    default_sym_filename: str

    @property
    def sym_url(self) -> str:
        return f"https://cdn.jsdelivr.net/gh/pret/{self.sym_repo}@symbols/{self.default_sym_filename}"


SUPPORTED_GAMES: dict[bytes, GameProfile] = {
    b"BPEE": GameProfile(
        game_code=b"BPEE",
        name="Pokemon Emerald",
        short_name="emerald",
        sym_repo="pokeemerald",
        default_sym_filename="pokeemerald.sym",
    ),
    b"BPRE": GameProfile(
        game_code=b"BPRE",
        name="Pokemon FireRed",
        short_name="firered",
        sym_repo="pokefirered",
        default_sym_filename="pokefirered.sym",
    ),
    b"BPGE": GameProfile(
        game_code=b"BPGE",
        name="Pokemon LeafGreen",
        short_name="leafgreen",
        sym_repo="pokefirered",
        default_sym_filename="pokefirered.sym",
    ),
    b"AXVE": GameProfile(
        game_code=b"AXVE",
        name="Pokemon Ruby",
        short_name="ruby",
        sym_repo="pokeruby",
        default_sym_filename="pokeruby.sym",
    ),
    b"AXPE": GameProfile(
        game_code=b"AXPE",
        name="Pokemon Sapphire",
        short_name="sapphire",
        sym_repo="pokeruby",
        default_sym_filename="pokeruby.sym",
    ),
}

ROM_REVISIONS: dict[bytes, dict[str, str]] = {
    b"AXVE": {
        "53d591215de2cab847d14fbcf8c516f0128cfa8556f1236065e0535aa5936d4e": "pokeruby.sym",
        "0d80909998a901c7edef5942068585bc855a85aec7e083aa6aeff84a5b2f8ec0": "pokeruby_rev1.sym",
        "0fdd36e92b75bed65d09df4635ab0b707b288c2bf1dc4c6e7a4a4f0eebe9d64c": "pokeruby_rev2.sym",
    },
    b"AXPE": {
        "c36c1b899503e8823ee7eb607eea583adcef7ea92ff804838b193c227f2c6657": "pokesapphire.sym",
        "2f680a43e5c57aede4cb3b2cb04f7e15079efc122c88edaacfd6026db6e920ac": "pokesapphire_rev1.sym",
        "02ca41513580a8b780989dee428df747b52a0b1a55bec617886b4059eb1152fb": "pokesapphire_rev2.sym",
    },
    b"BPEE": {
        "a9dec84dfe7f62ab2220bafaef7479da0929d066ece16a6885f6226db19085af": "pokeemerald.sym",
    },
    b"BPRE": {
        "3d0c79f1627022e18765766f6cb5ea067f6b5bf7dca115552189ad65a5c3a8ac": "pokefirered.sym",
        "729041b940afe031302d630fdbe57c0c145f3f7b6d9b8eca5e98678d0ca4d059": "pokefirered_rev1.sym",
    },
    b"BPGE": {
        "78d310d557ceebc593bd393acc52d1b19a8f023fec40bc200e6063880d8531fc": "pokeleafgreen.sym",
        "2f978f635b9593f6ca26ec42481c53a6b39f6cddd894ad5c062c1419fac58825": "pokeleafgreen_rev1.sym",
    },
}

SHA256_TO_GAME: dict[str, tuple[bytes, str]] = {
    sha256: (game_code, sym_filename)
    for game_code, revisions in ROM_REVISIONS.items()
    for sha256, sym_filename in revisions.items()
}


def compute_rom_sha256(rom_data: bytes) -> str:
    """Compute the SHA256 hash of a ROM file."""
    return sha256(rom_data).hexdigest()


@dataclass
class IdentifiedRom:
    """Result of ROM identification."""

    game: GameProfile
    sym_filename: str

    @property
    def sym_url(self) -> str:
        return f"https://cdn.jsdelivr.net/gh/pret/{self.game.sym_repo}@symbols/{self.sym_filename}"


def identify_rom(rom_data: bytes) -> IdentifiedRom | None:
    """
    Identify a ROM by its SHA256 hash.

    Args:
        rom_data: Raw ROM bytes.

    Returns:
        IdentifiedRom with game profile and sym filename if found, None otherwise.
    """
    rom_hash = compute_rom_sha256(rom_data)
    result = SHA256_TO_GAME.get(rom_hash)
    if result is None:
        return None
    game_code, sym_filename = result
    game = SUPPORTED_GAMES.get(game_code)
    if game is None:
        return None
    return IdentifiedRom(game=game, sym_filename=sym_filename)


def detect_game(rom_data: bytes) -> GameProfile | None:
    """Detect the game from ROM header game code."""
    if len(rom_data) < GAME_CODE_OFFSET + 4:
        return None
    game_code = rom_data[GAME_CODE_OFFSET : GAME_CODE_OFFSET + 4]
    return SUPPORTED_GAMES.get(game_code)


def detect_sym_filename(rom_data: bytes) -> str | None:
    """
    Detect the correct symbol filename based on ROM SHA256 hash.

    For games with multiple revisions (FireRed, LeafGreen), this function
    checks the ROM's SHA256 hash against known revisions to return the
    appropriate symbol filename.

    Args:
        rom_data: Raw ROM bytes.

    Returns:
        Symbol filename if found, None if ROM is unknown or has no revisions.
    """
    if len(rom_data) < GAME_CODE_OFFSET + 4:
        return None
    game_code = rom_data[GAME_CODE_OFFSET : GAME_CODE_OFFSET + 4]
    if game_code not in ROM_REVISIONS:
        return None
    rom_hash = compute_rom_sha256(rom_data)
    return ROM_REVISIONS[game_code].get(rom_hash)


def get_game_by_name(name: str) -> GameProfile | None:
    """Get game profile by name or short_name (case-insensitive)."""
    name_lower = name.lower()
    for game in SUPPORTED_GAMES.values():
        if game.name.lower() == name_lower or game.short_name.lower() == name_lower:
            return game
    return None


class OffsetType(Enum):
    """Symbol scope: GLOBAL for global symbols, LOCAL for local symbols."""

    GLOBAL = 0
    LOCAL = 1


@dataclass
class Offset:
    """Represents a symbol entry from a .sym file."""

    address: int
    type: OffsetType
    length: int
    name: str


@dataclass
class MapLayout:
    """Represents a map layout structure from the ROM."""

    width: int
    height: int
    border_ptr: int
    map_ptr: int
    primary_tileset_ptr: int
    secondary_tileset_ptr: int


@dataclass
class Tileset:
    """Represents a tileset structure from the ROM."""

    is_compressed: bool
    is_secondary: bool
    tiles_ptr: int
    palettes_ptr: int
    metatiles_ptr: int
    metatile_attributes_ptr: int
    callback_ptr: int
