//! Integration tests against real Pokemon ROMs.
//!
//! These tests are gated on the presence of `tests/fixtures/*.gba` files
//! (which are gitignored). They are skipped automatically when no ROM is
//! available.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use hypercutter::{Extractor, Rom, SymbolTable};

fn fixtures_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    dir.exists().then_some(dir)
}

fn load_rom(rom_name: &str) -> Option<(Rom, SymbolTable)> {
    let dir = fixtures_dir()?;
    let rom_path = dir.join(rom_name);
    if !rom_path.exists() {
        eprintln!("skipping: ROM not found at {}", rom_path.display());
        return None;
    }
    let rom = Rom::open(&rom_path).ok()?;
    let symbols = SymbolTable::resolve_for_rom(&rom)
        .expect("bundled symbol table must validate against fixture ROM");
    Some((rom, symbols))
}

#[test]
fn identifies_emerald_rom() {
    let Some((rom, _)) = load_rom("pokeemerald.gba") else {
        return;
    };
    assert_eq!(rom.game(), hypercutter::Game::Emerald);
}

#[test]
fn extracts_general_metatiles_from_emerald() {
    let Some((rom, symbols)) = load_rom("pokeemerald.gba") else {
        return;
    };
    let extractor = Extractor::new(&rom, &symbols);
    let metatiles = extractor.metatiles().expect("metatiles extraction");
    assert!(
        !metatiles.is_empty(),
        "expected at least one metatile entry"
    );
    // Emerald uses "General" for its primary metatile bank.
    assert!(metatiles.get("General").is_some(), "expected General");
}

#[test]
fn species_names_match_known_count() {
    let Some((rom, symbols)) = load_rom("pokeemerald.gba") else {
        return;
    };
    let extractor = Extractor::new(&rom, &symbols);
    let names = extractor.species_names().expect("species names");
    assert!(
        names.len() > 400,
        "expected 400+ species, got {}",
        names.len()
    );
    assert!(names.iter().any(|n| n == "pikachu"));
    assert!(names.iter().any(|n| n == "charizard"));
}

#[test]
fn extracts_pikachu_sprite() {
    let Some((rom, symbols)) = load_rom("pokeemerald.gba") else {
        return;
    };
    let extractor = Extractor::new(&rom, &symbols);
    let sprites = extractor.sprites().expect("sprites");
    assert!(sprites.iter().any(|s| s.name == "pikachu"));
    let pikachu = sprites.iter().find(|s| s.name == "pikachu").unwrap();
    assert!(pikachu.front.is_some());
    let front = pikachu.front.as_ref().unwrap();
    assert_eq!(front.tiles.tile_count(), 64);
}
