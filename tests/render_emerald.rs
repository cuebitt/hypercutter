//! Render-based integration tests against real Pokemon ROMs.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use hypercutter::{Extractor, Rom, SymbolTable, TilesetRenderer};

fn fixtures_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    dir.exists().then_some(dir)
}

fn load_rom(rom_name: &str) -> Option<(Rom, SymbolTable)> {
    let dir = fixtures_dir()?;
    let rom_path = dir.join(rom_name);
    if !rom_path.exists() {
        return None;
    }
    let rom = Rom::open(&rom_path).ok()?;
    let symbols = SymbolTable::resolve_for_rom(&rom).ok()?;
    Some((rom, symbols))
}

#[test]
fn renders_general_tileset_to_png() {
    let Some((rom, symbols)) = load_rom("pokeemerald.gba") else {
        return;
    };
    let extractor = Extractor::new(&rom, &symbols);
    let metatiles = extractor.metatiles().expect("metatiles");
    let Some(entry) = metatiles.get("General") else {
        return;
    };
    let renderer = TilesetRenderer::new(&entry.primary);
    let img = renderer.render();
    assert!(img.width() >= 16);
    assert!(img.height() >= 16);
    // Confirm at least one non-transparent pixel exists.
    let bytes = img.as_bytes();
    let has_color = bytes.chunks_exact(4).any(|px| px[3] != 0);
    assert!(has_color, "rendered tileset had no non-transparent pixels");
}

#[test]
fn rendered_png_writes_to_file() {
    let Some((rom, symbols)) = load_rom("pokeemerald.gba") else {
        return;
    };
    let dir = fixtures_dir().unwrap();
    let out_path = dir.join("general_tileset_test.png");
    let extractor = Extractor::new(&rom, &symbols);
    let metatiles = extractor.metatiles().expect("metatiles");
    if let Some(entry) = metatiles.get("General") {
        let renderer = TilesetRenderer::new(&entry.primary);
        let img = renderer.render();
        img.save_png(&out_path).expect("save png");
        assert!(out_path.exists());
    }
}
