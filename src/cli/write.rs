//! Output writers: PNG tilesets and sprite PNGs.

use std::path::Path;

use anyhow::{Context, Result};

use crate::{Extractor, SpriteRenderer, TilesetRenderer};

use super::Cli;

/// Run the full output pipeline.
pub(crate) fn run(
    cli: &Cli,
    extractor: &Extractor<'_>,
    dump_sprites: bool,
    dump_tilesets: bool,
) -> Result<()> {
    if dump_tilesets {
        let metatiles = extractor
            .metatiles()
            .with_context(|| "extracting metatiles")?;
        let tilesets_dir = cli.export.join("tilesets");
        output::write_tileset_pngs(&metatiles, &tilesets_dir, extractor.rom().game())?;
        log::info!("tileset PNGs written to {}", tilesets_dir.display());
    }
    if dump_sprites {
        let sprites = extractor.sprites().with_context(|| "extracting sprites")?;
        log::info!("extracted {} sprites", sprites.len());
        let species_names = extractor
            .species_names()
            .with_context(|| "loading species names")?;
        let national_map = extractor
            .national_dex_map()
            .with_context(|| "loading national dex map")?;
        let sprites_dir = cli.export.join("pokemon/sprites");
        output::write_sprites(&sprites, &species_names, &national_map, &sprites_dir, cli)?;
        // Also extract and write alternate forms.
        let forms = extractor.forms().with_context(|| "extracting forms")?;
        if !forms.is_empty() {
            log::info!("extracted {} form sprites", forms.len());
            output::write_forms(&forms, &species_names, &national_map, &sprites_dir)?;
        }
    }
    Ok(())
}

pub(crate) mod output {
    use super::*;
    use crate::{Metatiles, Sprite, SpriteSheet};

    pub(super) fn write_tileset_pngs(
        metatiles: &Metatiles,
        export_dir: &Path,
        game: crate::Game,
    ) -> Result<()> {
        std::fs::create_dir_all(export_dir)
            .with_context(|| format!("creating {}", export_dir.display()))?;
        let primary_tile_count = game.primary_tile_count();
        let exclude = game_exclude(game);
        for (name, entry) in metatiles.iter() {
            if exclude.contains(&name) {
                continue;
            }
            let renderer = match entry.secondary.as_ref() {
                Some(secondary) => TilesetRenderer::new(&entry.primary)
                    .with_secondary(secondary)
                    .with_primary_tile_count(primary_tile_count),
                None => {
                    TilesetRenderer::new(&entry.primary).with_primary_tile_count(primary_tile_count)
                }
            };
            let img = renderer.render();
            let path = export_dir.join(format!("{name}.png"));
            img.save_png(&path)
                .with_context(|| format!("saving {}", path.display()))?;
        }
        Ok(())
    }

    pub(super) fn write_sprites(
        sprites: &[Sprite],
        species_names: &[String],
        national_map: &[u16],
        out_dir: &Path,
        cli: &Cli,
    ) -> Result<()> {
        if cli.spritesheet {
            write_spritesheet(sprites, species_names, out_dir, cli.spritesheet_columns)
        } else {
            write_individual(sprites, species_names, national_map, out_dir)
        }
    }

    fn write_spritesheet(
        sprites: &[Sprite],
        _species_names: &[String],
        out_dir: &Path,
        columns: usize,
    ) -> Result<()> {
        std::fs::create_dir_all(out_dir)
            .with_context(|| format!("creating {}", out_dir.display()))?;
        let mut front_imgs = Vec::new();
        let mut back_imgs = Vec::new();
        let mut species_list = Vec::new();
        for sprite in sprites {
            if let Some(img) = render_sprite(sprite, true) {
                front_imgs.push(img);
                species_list.push(sprite.name.clone());
            }
            if let Some(img) = render_sprite(sprite, false) {
                back_imgs.push(img);
            }
        }
        if !front_imgs.is_empty() {
            let sheet = compose_spritesheet(&front_imgs, columns);
            sheet
                .save_png(out_dir.join("front_spritesheet.png"))
                .with_context(|| "saving front spritesheet")?;
        }
        if !back_imgs.is_empty() {
            let sheet = compose_spritesheet(&back_imgs, columns);
            sheet
                .save_png(out_dir.join("back_spritesheet.png"))
                .with_context(|| "saving back spritesheet")?;
        }
        let list = species_list
            .into_iter()
            .enumerate()
            .map(|(i, name)| format!("{}: {}", i + 1, name))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(out_dir.join("species_list.txt"), list)
            .with_context(|| "writing species_list.txt")?;
        Ok(())
    }

    fn write_individual(
        sprites: &[Sprite],
        _species_names: &[String],
        national_map: &[u16],
        out_dir: &Path,
    ) -> Result<()> {
        std::fs::create_dir_all(out_dir)
            .with_context(|| format!("creating {}", out_dir.display()))?;
        for sprite in sprites {
            let display_name = if sprite.name == "?" || sprite.name.is_empty() {
                "egg"
            } else {
                &sprite.name
            };
            // Use national dex number for the directory prefix when
            // available, falling back to the internal species ID.
            let dex_num = national_map
                .get(sprite.id.0 as usize)
                .copied()
                .filter(|&n| n != 0)
                .unwrap_or(sprite.id.0);
            let species_dir = if display_name == "egg" {
                out_dir.join("egg")
            } else {
                out_dir.join(format!("{:03}_{}", dex_num, display_name))
            };
            std::fs::create_dir_all(&species_dir)
                .with_context(|| format!("creating {}", species_dir.display()))?;
            for (is_front, label) in [(true, "front"), (false, "back")] {
                for (is_shiny, suffix) in [(false, ""), (true, "_shiny")] {
                    if let Some(img) = render_sprite_variant(sprite, is_front, is_shiny) {
                        let path = species_dir.join(format!("{label}{suffix}.png"));
                        img.save_png(&path)
                            .with_context(|| format!("saving {}", path.display()))?;
                    }
                }
            }
        }
        Ok(())
    }

    fn render_sprite(sprite: &Sprite, is_front: bool) -> Option<crate::RgbaImage> {
        render_sprite_variant(sprite, is_front, false)
    }

    fn render_sprite_variant(
        sprite: &Sprite,
        is_front: bool,
        is_shiny: bool,
    ) -> Option<crate::RgbaImage> {
        let sheet: &SpriteSheet = if is_front {
            sprite.front.as_ref()?
        } else {
            sprite.back.as_ref()?
        };
        let palette_data = if is_shiny {
            &sprite.shiny_palette
        } else {
            &sprite.palette
        };
        let palette = palette_data.get(0)?;
        Some(SpriteRenderer::new(sheet, palette).render())
    }

    pub(super) fn write_forms(
        forms: &[crate::FormSprite],
        species_names: &[String],
        national_map: &[u16],
        sprites_dir: &Path,
    ) -> Result<()> {
        use crate::sprite::MonCoords;
        use crate::tileset::TileData;
        use crate::{FormSprite, SpriteRenderer, SpriteSheet};
        use std::collections::BTreeMap;

        // Group forms by base species.
        let mut by_species: BTreeMap<u16, Vec<&FormSprite>> = BTreeMap::new();
        for form in forms {
            by_species.entry(form.base.0).or_default().push(form);
        }

        for (base_id, form_list) in &by_species {
            let base_name = species_names
                .get(*base_id as usize)
                .cloned()
                .unwrap_or_default();
            if base_name.is_empty() || base_name == "?" {
                continue;
            }
            let dex_num = national_map
                .get(*base_id as usize)
                .copied()
                .filter(|&n| n != 0)
                .unwrap_or(*base_id);
            let species_dir = sprites_dir.join(format!("{:03}_{}", dex_num, base_name));
            let forms_dir = species_dir.join("forms");

            for form in form_list {
                let form_dir = forms_dir.join(&form.form);
                std::fs::create_dir_all(&form_dir)
                    .with_context(|| format!("creating {}", form_dir.display()))?;

                for (_is_front, label, tiles_data) in [
                    (true, "front", form.front_tiles.as_ref()),
                    (false, "back", form.back_tiles.as_ref()),
                ] {
                    let Some(tiles) = tiles_data else {
                        continue;
                    };
                    let sheet = SpriteSheet {
                        tiles: TileData::from_bytes(tiles.clone()),
                        coords: MonCoords::default(),
                    };
                    for (_is_shiny, suffix, palette_opt) in [
                        (false, "", form.palette.as_ref()),
                        (true, "_shiny", form.shiny_palette.as_ref()),
                    ] {
                        let Some(pal) = palette_opt else {
                            continue;
                        };
                        let Some(palette_data) = pal.get(0) else {
                            continue;
                        };
                        let img = SpriteRenderer::new(&sheet, palette_data).render();
                        let path = form_dir.join(format!("{label}{suffix}.png"));
                        img.save_png(&path)
                            .with_context(|| format!("saving {}", path.display()))?;
                    }
                }
            }
        }
        Ok(())
    }

    fn compose_spritesheet(images: &[crate::RgbaImage], columns: usize) -> crate::RgbaImage {
        let max_w = images.iter().map(|i| i.width()).max().unwrap_or(64);
        let max_h = images.iter().map(|i| i.height()).max().unwrap_or(64);
        let rows = images.len().div_ceil(columns);
        let pad = 1u32;
        let sheet_w = columns as u32 * (max_w + pad) - pad;
        let sheet_h = rows as u32 * (max_h + pad) - pad;
        let mut sheet = crate::RgbaImage::new(sheet_w, sheet_h);
        for (idx, img) in images.iter().enumerate() {
            let col = idx % columns;
            let row = idx / columns;
            let x = col as u32 * (max_w + pad);
            let y = row as u32 * (max_h + pad);
            sheet.alpha_blit(img, (x, y));
        }
        sheet
    }

    fn game_exclude(game: crate::Game) -> &'static [&'static str] {
        match game {
            crate::Game::FireRed | crate::Game::LeafGreen => &["HoennBuilding"],
            _ => &[],
        }
    }
}
