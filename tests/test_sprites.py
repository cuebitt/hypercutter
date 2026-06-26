"""Tests for Pokemon sprite extraction functionality."""

import struct

import pytest

from hypercutter.classes import MonCoords, SpritePalette, SpriteSheet
from hypercutter.extractors import (
    extract_mon_coords,
    extract_sprite_palette,
    extract_sprite_sheet,
)
from hypercutter.sprite_renderer import PokemonSpriteRenderer, get_species_name


class TestSpriteSheet:
    def test_extracts_correct_values(self):
        data = struct.pack("<IHH", 0x08100000, 0x1000, 0x0001)
        result = extract_sprite_sheet(data, 0)
        assert isinstance(result, SpriteSheet)
        assert result.data_ptr == 0x08100000
        assert result.size == 0x1000
        assert result.tag == 0x0001

    def test_raises_on_invalid_offset(self):
        data = bytes(0x10)
        with pytest.raises(ValueError, match="out of range"):
            extract_sprite_sheet(data, 0x100)


class TestSpritePalette:
    def test_extracts_correct_values(self):
        data = struct.pack("<IHH", 0x08200000, 0x0001, 0x0000)
        result = extract_sprite_palette(data, 0)
        assert isinstance(result, SpritePalette)
        assert result.data_ptr == 0x08200000
        assert result.tag == 0x0001

    def test_raises_on_invalid_offset(self):
        data = bytes(0x10)
        with pytest.raises(ValueError, match="out of range"):
            extract_sprite_palette(data, 0x100)


class TestMonCoords:
    def test_extracts_correct_values(self):
        # size = 0x45 means width=4 tiles (32px), height=5 tiles (40px)
        # 4 bytes: size, y_offset, padding, padding
        data = struct.pack("<BBBB", 0x45, 14, 0x00, 0x00)
        result = extract_mon_coords(data, 0)
        assert isinstance(result, MonCoords)
        assert result.size == 0x45
        assert result.y_offset == 14

    def test_width_pixels(self):
        data = struct.pack("<BBBB", 0x45, 0, 0x00, 0x00)  # width=4 tiles
        result = extract_mon_coords(data, 0)
        assert result.width_pixels == 32  # 4 * 8

    def test_height_pixels(self):
        data = struct.pack("<BBBB", 0x45, 0, 0x00, 0x00)  # height=5 tiles
        result = extract_mon_coords(data, 0)
        assert result.height_pixels == 40  # 5 * 8

    def test_raises_on_invalid_offset(self):
        data = bytes(0x10)
        with pytest.raises(ValueError, match="out of range"):
            extract_mon_coords(data, 0x100)


class TestSpriteRenderer:
    def test_renders_64x64_image(self):
        # Full 64x64 frame = 64 tiles * 32 bytes = 2048 bytes
        tile_data = bytes([0x11] * 32) + b"\x00" * (2048 - 32)
        palette_data = struct.pack("<HH", 0x0000, 0x001F) + b"\x00" * 28

        renderer = PokemonSpriteRenderer(tile_data, palette_data)
        img = renderer.render()
        assert img.size == (64, 64)
        assert img.mode == "RGBA"

    def test_respects_transparency(self):
        tile_data = b"\x00" * 2048
        palette_data = struct.pack("<HH", 0x7C00, 0x001F) + b"\x00" * 28

        renderer = PokemonSpriteRenderer(tile_data, palette_data)
        img = renderer.render(is_transparent=True)
        pixel = img.getpixel((0, 0))
        assert pixel[3] == 0  # type: ignore[index]  # Transparent  # ty:ignore[not-subscriptable]

    def test_decode_palette(self):
        palette_data = struct.pack("<HH", 0x7FFF, 0x001F) + b"\x00" * 28

        renderer = PokemonSpriteRenderer(b"", palette_data)
        palette = renderer.decode_palette()
        assert palette[0] == (255, 255, 255)  # White
        assert palette[1] == (255, 0, 0)  # Red


class TestGetSpeciesName:
    def test_known_species(self):
        species_names = [
            "missing",
            "bulbasaur",
            "ivysaur",
            "venusaur",
            "charmander",
            "charmeleon",
            "charizard",
            "squirtle",
            "wartortle",
            "blastoise",
            "caterpie",
            "metapod",
            "butterfree",
            "weedle",
            "kakuna",
            "beedrill",
            "pidgey",
            "pidgeotto",
            "pidgeot",
            "rattata",
            "raticate",
            "spearow",
            "fearow",
            "ekans",
            "arbok",
            "pikachu",
            "raichu",
            "sandshrew",
            "sandslash",
            "nidoran_f",
            "nidorina",
            "nidoqueen",
            "nidoran_m",
            "nidorino",
            "nidoking",
            "clefairy",
            "clefable",
            "vulpix",
            "ninetales",
            "jigglypuff",
            "wigglytuff",
            "zubat",
            "golbat",
            "oddish",
            "gloom",
            "vileplume",
            "paras",
            "parasect",
            "venonat",
            "venomoth",
            "diglett",
            "dugtrio",
            "meowth",
            "persian",
            "psyduck",
            "golduck",
            "mankey",
            "primeape",
            "growlithe",
            "arcanine",
            "poliwag",
            "poliwhirl",
            "poliwrath",
            "abra",
            "kadabra",
            "alakazam",
            "machop",
            "machoke",
            "machamp",
            "bellsprout",
            "weepinbell",
            "victreebel",
            "tentacool",
            "tentacruel",
            "geodude",
            "graveler",
            "golem",
            "ponyta",
            "rapidash",
            "slowpoke",
            "slowbro",
            "magnemite",
            "magneton",
            "farfetchd",
            "doduo",
            "dodrio",
            "seel",
            "dewgong",
            "grimer",
            "muk",
            "shellder",
            "cloyster",
            "gastly",
            "haunter",
            "gengar",
            "onix",
            "drowzee",
            "hypno",
            "krabby",
            "kingler",
            "voltorb",
            "electrode",
            "exeggcute",
            "exeggutor",
            "cubone",
            "marowak",
            "hitmonlee",
            "hitmonchan",
            "lickitung",
            "koffing",
            "weezing",
            "rhyhorn",
            "rhydon",
            "chansey",
            "tangela",
            "kangaskhan",
            "horsea",
            "seadra",
            "goldeen",
            "seaking",
            "staryu",
            "starmie",
            "mr_mime",
            "scyther",
            "jynx",
            "electabuzz",
            "magmar",
            "pinsir",
            "tauros",
            "magikarp",
            "gyarados",
            "lapras",
            "ditto",
            "eevee",
            "vaporeon",
            "jolteon",
            "flareon",
            "porygon",
            "omanyte",
            "omastar",
            "kabuto",
            "kabutops",
            "aerodactyl",
            "snorlax",
            "articuno",
            "zapdos",
            "moltres",
            "dratini",
            "dragonair",
            "dragonite",
            "mewtwo",
            "mew",
            "chikorita",
            "bayleef",
            "meganium",
            "cyndaquil",
            "quilava",
            "typhlosion",
            "totodile",
            "croconaw",
            "feraligatr",
            "sentret",
            "furret",
            "hoothoot",
            "noctowl",
            "ledyba",
            "ledian",
            "spinarak",
            "ariados",
            "crobat",
            "chinchou",
            "lanturn",
            "pichu",
            "cleffa",
            "igglybuff",
            "togepi",
            "togetic",
            "natu",
            "xatu",
            "mareep",
            "flaaffy",
            "ampharos",
            "bellossom",
            "marill",
            "azumarill",
            "sudowoodo",
            "politoed",
            "hoppip",
            "skiploom",
            "jumpluff",
            "aipom",
            "sunkern",
            "sunflora",
            "yanma",
            "wooper",
            "quagsire",
            "espeon",
            "umbreon",
            "murkrow",
            "slowking",
            "misdreavus",
            "unown",
            "wobbuffet",
            "girafarig",
            "pineco",
            "forretress",
            "dunsparce",
            "gligar",
            "steelix",
            "snubbull",
            "granbull",
            "qwilfish",
            "scizor",
            "shuckle",
            "heracross",
            "sneasel",
            "teddiursa",
            "ursaring",
            "slugma",
            "magcargo",
            "swinub",
            "piloswine",
            "corsola",
            "remoraid",
            "octillery",
            "delibird",
            "mantine",
            "skarmory",
            "houndour",
            "houndoom",
            "kingdra",
            "phanpy",
            "donphan",
            "porygon2",
            "stantler",
            "smeargle",
            "tyrogue",
            "hitmontop",
            "smoochum",
            "elekid",
            "magby",
            "miltank",
            "blissey",
            "raikou",
            "entei",
            "suicune",
            "larvitar",
            "pupitar",
            "tyranitar",
            "lugia",
            "ho_oh",
            "celebi",
            "old_unown_b",
            "old_unown_c",
            "old_unown_d",
            "old_unown_e",
            "old_unown_f",
            "old_unown_g",
            "old_unown_h",
            "old_unown_i",
            "old_unown_j",
            "old_unown_k",
            "old_unown_l",
            "old_unown_m",
            "old_unown_n",
            "old_unown_o",
            "old_unown_p",
            "old_unown_q",
            "old_unown_r",
            "old_unown_s",
            "old_unown_t",
            "old_unown_u",
            "old_unown_v",
            "old_unown_w",
            "old_unown_x",
            "old_unown_y",
            "old_unown_z",
            "treecko",
            "grovyle",
            "sceptile",
            "torchic",
            "combusken",
            "blaziken",
            "mudkip",
            "marshtomp",
            "swampert",
            "poochyena",
            "mightyena",
            "zigzagoon",
            "linoone",
            "wurmple",
            "silcoon",
            "beautifly",
            "cascoon",
            "dustox",
            "lotad",
            "lombre",
            "ludicolo",
            "seedot",
            "nuzleaf",
            "shiftry",
            "nincada",
            "ninjask",
            "shedinja",
            "tallow",
            "swellow",
            "shroomish",
            "breloom",
            "spinda",
            "wingull",
            "pelipper",
            "surskit",
            "masquerain",
            "wailmer",
            "wailord",
            "skitty",
            "delcatty",
            "kecleon",
            "baltoy",
            "claydol",
            "nosepass",
            "torkoal",
            "sableye",
            "barboach",
            "whiscash",
            "luvdisc",
            "corphish",
            "crawdaunt",
            "feebas",
            "milotic",
            "carvanha",
            "sharpedo",
            "trapinch",
            "vibrava",
            "flygon",
            "makuhita",
            "hariyama",
            "electrike",
            "manectric",
            "numel",
            "camerupt",
            "spheal",
            "sealeo",
            "walrein",
            "cacnea",
            "cacturne",
            "snorunt",
            "glalie",
            "lunatone",
            "solrock",
            "azurill",
            "spoink",
            "grumpig",
            "plusle",
            "minun",
            "mawile",
            "meditite",
            "medicham",
            "swablu",
            "altaria",
            "wynaut",
            "duskull",
            "dusclops",
            "roselia",
            "slakoth",
            "vigoroth",
            "slaking",
            "gulpin",
            "swalot",
            "tropius",
            "whismur",
            "loudred",
            "exploud",
            "clamperl",
            "huntail",
            "gorebyss",
            "absol",
            "shuppet",
            "banette",
            "seviper",
            "zangoose",
            "relicanth",
            "aron",
            "lairon",
            "aggron",
            "castform",
            "volbeat",
            "illumise",
            "lileep",
            "cradily",
            "anorith",
            "armaldo",
            "ralts",
            "kirlia",
            "gardevoir",
            "bagon",
            "shelgon",
            "salamence",
            "beldum",
            "metang",
            "metagross",
            "regirock",
            "regice",
            "registeel",
            "latias",
            "latios",
            "kyogre",
            "groudon",
            "rayquaza",
            "jirachi",
            "deoxys",
            "chimecho",
            "egg",
        ]
        assert get_species_name(0, species_names) == "missing"
        assert get_species_name(1, species_names) == "bulbasaur"
        assert get_species_name(25, species_names) == "pikachu"
        assert get_species_name(410, species_names) == "deoxys"

    def test_unknown_species(self):
        assert get_species_name(0) == "unknown_000"
        assert get_species_name(999) == "unknown_999"
