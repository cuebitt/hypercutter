//! Generator for bundled TOML symbol tables.
//!
//! Downloads `.sym` files from the pret GitHub repos via jsDelivr,
//! filters to symbols that hypercutter actually uses, and writes
//! grouped TOML files under `symbols/`.
//!
//! Usage:
//! ```text
//! cargo run --example gen_symbols
//! ```

use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use hypercutter::SymbolTable;

/// (toml_filename, repo, sym_filename[, revision_sym_filenames…])
const GAMES: &[(&str, &str, &str, &[&str])] = &[
    ("emerald", "pokeemerald", "pokeemerald.sym", &[]),
    (
        "firered",
        "pokefirered",
        "pokefirered.sym",
        &["pokefirered_rev1.sym"],
    ),
    // LeafGreen shares the pokefirered repo (same codebase)
    (
        "leafgreen",
        "pokefirered",
        "pokefirered.sym",
        &["pokeleafgreen_rev1.sym"],
    ),
    (
        "ruby",
        "pokeruby",
        "pokeruby.sym",
        &["pokeruby_rev1.sym", "pokeruby_rev2.sym"],
    ),
    // Sapphire shares the pokeruby repo (same codebase)
    (
        "sapphire",
        "pokeruby",
        "pokeruby.sym",
        &["pokesapphire_rev1.sym", "pokesapphire_rev2.sym"],
    ),
];

fn sym_url(repo: &str, file: &str) -> String {
    format!("https://cdn.jsdelivr.net/gh/pret/{repo}@symbols/{file}")
}

fn download_sym(repo: &str, file: &str) -> Result<String> {
    let url = sym_url(repo, file);
    eprintln!("downloading {url}");
    let body = ureq::get(&url).call()?.into_body().read_to_string()?;
    Ok(body)
}

struct TilesetEntry {
    offset: u32,
    tiles: Option<u32>,
    palettes: Option<u32>,
}

struct GameData {
    start: u32,
    tables: Vec<(String, u32)>,
    tilesets: BTreeMap<String, TilesetEntry>,
    metatiles: Vec<(String, u32, u32)>,
    pokemon_tables: Vec<(String, u32)>,
    field_sprites: Vec<(String, u32)>,
    field_sprite_palettes: Vec<(String, u32)>,
    field_effects: Vec<(String, u32, u32)>, // (name, address, length)
    pokemon_components: BTreeMap<String, BTreeMap<String, u32>>,
}

fn group_symbols(symbols: &SymbolTable) -> GameData {
    let start = symbols
        .get("Start")
        .map(|s| s.address)
        .unwrap_or(0x0800_0000);
    eprintln!("  start = 0x{start:08x}");
    let mut data = GameData {
        start,
        tables: Vec::new(),
        tilesets: BTreeMap::new(),
        metatiles: Vec::new(),
        pokemon_tables: Vec::new(),
        field_sprites: Vec::new(),
        field_sprite_palettes: Vec::new(),
        field_effects: Vec::new(),
        pokemon_components: BTreeMap::new(),
    };

    // Pass 1: collect all symbols into buckets.
    let mut tileset_helpers: BTreeMap<String, (Option<u32>, Option<u32>)> = BTreeMap::new();

    for sym in symbols.iter() {
        let name = sym.name.as_str();
        let addr = sym.address;

        // Tables (Start is included as a table entry)
        if name == "Start" || name == "start" {
            data.tables.push((name.to_owned(), addr));
            continue;
        }
        let base_tables = [
            "gMapLayouts",
            "gSpeciesNames",
            "sSpeciesToNationalPokedexNum",
            "SpeciesToNationalPokedexNum",
            "gSpeciesToNationalPokedexNum",
            "sObjectEventSpritePalettes",
        ];
        if base_tables.contains(&name) {
            data.tables.push((name.to_owned(), addr));
            continue;
        }

        // Pokemon tables
        let pokemon_table_names = [
            "gMonFrontPicTable",
            "gMonBackPicTable",
            "gMonPaletteTable",
            "gMonShinyPaletteTable",
            "gMonFrontPicCoords",
            "gMonBackPicCoords",
            "gMonFootprintTable",
        ];
        if pokemon_table_names.contains(&name) {
            data.pokemon_tables.push((name.to_owned(), addr));
            continue;
        }

        // Field sprite table
        if name == "gObjectEventGraphicsInfoPointers" {
            data.field_sprites.push((name.to_owned(), addr));
            continue;
        }

        // Field sprite palettes
        if let Some(stripped) = name.strip_prefix("gObjectEventPal_") {
            data.field_sprite_palettes.push((stripped.to_owned(), addr));
            continue;
        }

        // Field effect pics (surf blob, shadows, grass effects, etc.)
        if name.starts_with("gFieldEffectObjectPic_")
            || name.starts_with("gFieldEffectPic_")
            || name == "gObjectEventPic_SurfBlob"
        {
            data.field_effects.push((name.to_owned(), addr, sym.length));
            continue;
        }
        // Field effect templates, anim tables, sprite palettes,
        // direct palette data, and Ruby's palette info entries.
        if name.starts_with("gFieldEffectObjectTemplate_")
            || name.starts_with("gFieldEffectSpriteTemplate_")
            || name.starts_with("gFieldEffectAnimTable_")
            || name.starts_with("gSpritePalette_")
            || name.starts_with("gFieldEffectPal_")
            || name.starts_with("gFieldEffectObjectPaletteInfo")
        {
            data.tables.push((name.to_owned(), addr));
            continue;
        }

        // Tileset helpers (process before tilesets so they don't collide)
        if let Some(stripped) = name.strip_prefix("gTilesetTiles_") {
            tileset_helpers.entry(stripped.to_owned()).or_default().0 = Some(addr);
            continue;
        }
        if let Some(stripped) = name.strip_prefix("gTilesetPalettes_") {
            tileset_helpers.entry(stripped.to_owned()).or_default().1 = Some(addr);
            continue;
        }

        // Tilesets
        if let Some(stripped) = name.strip_prefix("gTileset_") {
            if !data.tilesets.contains_key(stripped) {
                let (tiles, palettes) = tileset_helpers.remove(stripped).unwrap_or_default();
                data.tilesets.insert(
                    stripped.to_owned(),
                    TilesetEntry {
                        offset: addr,
                        tiles,
                        palettes,
                    },
                );
            }
            continue;
        }

        // Metatiles
        if let Some(stripped) = name.strip_prefix("gMetatiles_") {
            data.metatiles.push((stripped.to_owned(), addr, sym.length));
            continue;
        }

        // Pokemon components
        let pokemon_prefixes = [
            ("gMonFrontPic_", "front"),
            ("gMonBackPic_", "back"),
            ("gMonPalette_", "palette"),
            ("gMonShinyPalette_", "shiny_palette"),
            ("gMonFootprint_", "footprint"),
        ];
        for &(prefix, component) in &pokemon_prefixes {
            if let Some(species) = name.strip_prefix(prefix) {
                data.pokemon_components
                    .entry(species.to_owned())
                    .or_default()
                    .insert(component.to_owned(), addr);
                break;
            }
        }

        // gObjectEventGraphicsInfo_* symbols
        if let Some(stripped) = name.strip_prefix("gObjectEventGraphicsInfo_") {
            data.field_sprites.push((stripped.to_owned(), addr));
        }
    }

    // Any remaining tileset helpers without a primary tileset? Rare but possible.
    // Just insert them with a placeholder offset for the primary.
    for (k, (tiles, palettes)) in &tileset_helpers {
        data.tilesets.entry(k.clone()).or_insert(TilesetEntry {
            offset: 0,
            tiles: *tiles,
            palettes: *palettes,
        });
    }

    data
}

fn emit_toml(data: &GameData, game: &str, revision: &str) -> String {
    let mut out = String::new();
    let start = data.start;

    writeln!(out, "game = {game:?}").unwrap();
    writeln!(out, "revision = {revision:?}").unwrap();
    writeln!(out, "start = {start}").unwrap();

    // Helper: subtract start, return offset or skip if below start.
    let rel = |addr: u32| -> Option<u32> {
        if addr < start {
            None
        } else {
            Some(addr - start)
        }
    };

    // Tables
    for (name, addr) in &data.tables {
        let Some(offset) = rel(*addr) else { continue };
        writeln!(out, "\n[[tables]]\nname = \"{name}\"\noffset = {offset}").unwrap();
    }

    // Tilesets
    if !data.tilesets.is_empty() {
        writeln!(out).unwrap();
        for (name, entry) in &data.tilesets {
            let Some(offset) = rel(entry.offset) else {
                continue;
            };
            write!(
                out,
                "[[tilesets]]\nname = \"gTileset_{name}\"\noffset = {offset}"
            )
            .unwrap();
            if let Some(t) = entry.tiles {
                if let Some(to) = rel(t) {
                    write!(out, "\ntiles = {to}").unwrap();
                }
            }
            if let Some(p) = entry.palettes {
                if let Some(po) = rel(p) {
                    write!(out, "\npalettes = {po}").unwrap();
                }
            }
            writeln!(out).unwrap();
        }
    }

    // Metatiles
    for (name, addr, length) in &data.metatiles {
        let Some(offset) = rel(*addr) else { continue };
        write!(
            out,
            "\n[[metatiles]]\nname = \"gMetatiles_{name}\"\noffset = {offset}"
        )
        .unwrap();
        if *length != 0 {
            write!(out, "\nlength = {length}").unwrap();
        }
        writeln!(out).unwrap();
    }

    // Pokemon tables
    for (name, addr) in &data.pokemon_tables {
        let Some(offset) = rel(*addr) else { continue };
        writeln!(
            out,
            "\n[[pokemon_tables]]\nname = \"{name}\"\noffset = {offset}"
        )
        .unwrap();
    }

    // Field sprites
    for (name, addr) in &data.field_sprites {
        let Some(offset) = rel(*addr) else { continue };
        let sym = if name.starts_with("gObjectEventGraphicsInfoPointers") {
            name.clone()
        } else {
            format!("gObjectEventGraphicsInfo_{name}")
        };
        writeln!(
            out,
            "\n[[field_sprites]]\nname = \"{sym}\"\noffset = {offset}"
        )
        .unwrap();
    }

    // Field sprite palettes
    for (name, addr) in &data.field_sprite_palettes {
        let Some(offset) = rel(*addr) else { continue };
        writeln!(
            out,
            "\n[[field_sprite_palettes]]\nname = \"gObjectEventPal_{name}\"\noffset = {offset}"
        )
        .unwrap();
    }

    // Field effects (surf blob, shadows, grass, etc.)
    for (name, addr, length) in &data.field_effects {
        let Some(offset) = rel(*addr) else { continue };
        write!(
            out,
            "\n[[field_effects]]\nname = \"{name}\"\noffset = {offset}"
        )
        .unwrap();
        if *length != 0 {
            write!(out, "\nlength = {length}").unwrap();
        }
        writeln!(out).unwrap();
    }

    // Pokemon components
    for (species, components) in &data.pokemon_components {
        writeln!(out, "\n[pokemon.{species}]").unwrap();
        for (component, addr) in components {
            let Some(offset) = rel(*addr) else { continue };
            writeln!(out, "{component} = {offset}").unwrap();
        }
    }

    out
}

fn main() -> Result<()> {
    let symbols_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("symbols");

    for (game, repo, default_file, revisions) in GAMES {
        let text = download_sym(repo, default_file)?;
        let symbols = SymbolTable::from_text(&text)?;
        let data = group_symbols(&symbols);
        let toml = emit_toml(&data, game, "");

        let path = symbols_dir.join(format!("{game}.toml"));
        fs::write(&path, &toml).with_context(|| format!("writing {}", path.display()))?;
        eprintln!("wrote {}", path.display());

        // Revisions
        for (i, rev_file) in revisions.iter().enumerate() {
            let rev_num = i + 1;
            let text = download_sym(repo, rev_file)?;
            let symbols = SymbolTable::from_text(&text)?;
            let data = group_symbols(&symbols);
            let toml = emit_toml(&data, game, &rev_num.to_string());

            let path = symbols_dir.join(format!("{game}_rev{rev_num}.toml"));
            fs::write(&path, &toml).with_context(|| format!("writing {}", path.display()))?;
            eprintln!("wrote {}", path.display());
        }
    }

    eprintln!("done");
    Ok(())
}
