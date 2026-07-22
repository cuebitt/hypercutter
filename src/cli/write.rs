//! Output writers: PNG tilesets and sprite PNGs.

use std::path::Path;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use crate::{render_footprint, SpriteRenderer, TilesetRenderer};

use super::Cli;

pub(crate) mod output {
    use super::*;
    use crate::{Metatiles, Sprite, SpriteSheet};

    /// Replace characters unsafe for filesystem paths with underscores.
    fn sanitize_name(name: &str) -> String {
        name.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    fn progress_bar(len: u64, quiet: bool) -> ProgressBar {
        if quiet {
            return ProgressBar::hidden();
        }
        let pb = ProgressBar::new(len);
        pb.set_style(
            ProgressStyle::with_template("  {spinner:.dim} [{bar:30}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb
    }

    pub(crate) fn write_tileset_pngs(
        metatiles: &Metatiles,
        export_dir: &Path,
        game: crate::Game,
        cli: &Cli,
    ) -> Result<usize> {
        std::fs::create_dir_all(export_dir)
            .with_context(|| format!("creating {}", export_dir.display()))?;
        let primary_tile_count = game.primary_tile_count();
        let exclude = super::super::game_exclude(game);
        let filter = cli
            .tileset_filter
            .as_deref()
            .map(glob::Pattern::new)
            .transpose()
            .with_context(|| "invalid tileset filter pattern")?;

        let entries: Vec<_> = metatiles
            .iter()
            .filter(|(name, _)| !exclude.contains(name))
            .filter(|(name, _)| {
                if let Some(ref pat) = filter {
                    pat.matches(name)
                } else {
                    true
                }
            })
            .collect();

        let pb = progress_bar(entries.len() as u64, cli.quiet);
        let count = entries.len();

        entries.par_iter().try_for_each(|(name, entry)| {
            let renderer = match entry.secondary.as_ref() {
                Some(secondary) => TilesetRenderer::new(&entry.primary)
                    .with_secondary(secondary)
                    .with_primary_tile_count(primary_tile_count),
                None => {
                    TilesetRenderer::new(&entry.primary).with_primary_tile_count(primary_tile_count)
                }
            };
            let tileset_dir = export_dir.join(name);
            std::fs::create_dir_all(&tileset_dir)
                .with_context(|| format!("creating {}", tileset_dir.display()))?;
            renderer
                .render()
                .save_png(tileset_dir.join("combined.png"))
                .with_context(|| {
                    format!("saving {}", tileset_dir.join("combined.png").display())
                })?;
            renderer
                .render_bottom()
                .save_png(tileset_dir.join("bottom.png"))
                .with_context(|| format!("saving {}", tileset_dir.join("bottom.png").display()))?;
            renderer
                .render_top()
                .save_png(tileset_dir.join("top.png"))
                .with_context(|| format!("saving {}", tileset_dir.join("top.png").display()))?;
            pb.inc(1);
            Ok::<(), anyhow::Error>(())
        })?;

        pb.finish_and_clear();
        Ok(count)
    }

    pub(crate) fn write_sprites(
        sprites: &[Sprite],
        national_map: &[u16],
        out_dir: &Path,
        cli: &Cli,
    ) -> Result<usize> {
        write_individual(sprites, national_map, out_dir, cli)
    }

    fn write_individual(
        sprites: &[Sprite],
        national_map: &[u16],
        out_dir: &Path,
        cli: &Cli,
    ) -> Result<usize> {
        std::fs::create_dir_all(out_dir)
            .with_context(|| format!("creating {}", out_dir.display()))?;

        let filter = cli
            .sprite_filter
            .as_deref()
            .map(glob::Pattern::new)
            .transpose()
            .with_context(|| "invalid sprite filter pattern")?;

        let filtered: Vec<_> = sprites
            .iter()
            .filter(|s| {
                if let Some(ref pat) = filter {
                    pat.matches(&s.name)
                } else {
                    true
                }
            })
            .collect();

        let pb = progress_bar(filtered.len() as u64 * 5, cli.quiet);
        let count = filtered.len();

        filtered.par_iter().try_for_each(|sprite| {
            let display_name = if sprite.name == "?" || sprite.name.is_empty() {
                "egg".to_owned()
            } else {
                sanitize_name(&sprite.name)
            };
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
                    pb.inc(1);
                }
            }
            if let Some(fp) = &sprite.footprint {
                let img = render_footprint(fp);
                let path = species_dir.join("footprint.png");
                img.save_png(&path)
                    .with_context(|| format!("saving {}", path.display()))?;
            }
            Ok::<(), anyhow::Error>(())
        })?;

        pb.finish_and_clear();
        Ok(count)
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

    pub(crate) fn write_forms(
        forms: &[crate::FormSprite],
        species_names: &[String],
        national_map: &[u16],
        sprites_dir: &Path,
        cli: &Cli,
    ) -> Result<usize> {
        use crate::sprite::MonCoords;
        use crate::tileset::TileData;
        use crate::{FormSprite, SpriteRenderer, SpriteSheet};
        use std::collections::BTreeMap;

        // Group forms by base species.
        let mut by_species: BTreeMap<u16, Vec<&FormSprite>> = BTreeMap::new();
        for form in forms {
            by_species.entry(form.base.0).or_default().push(form);
        }

        let filter = cli
            .sprite_filter
            .as_deref()
            .map(glob::Pattern::new)
            .transpose()
            .with_context(|| "invalid sprite filter pattern")?;

        // Flatten into a Vec for parallel processing.
        let mut work: Vec<(&FormSprite, std::path::PathBuf)> = Vec::new();
        for (base_id, form_list) in &by_species {
            let base_name = species_names
                .get(*base_id as usize)
                .cloned()
                .unwrap_or_default();
            if base_name.is_empty() || base_name == "?" {
                continue;
            }
            if let Some(ref pat) = filter {
                if !pat.matches(&base_name) {
                    continue;
                }
            }
            let dex_num = national_map
                .get(*base_id as usize)
                .copied()
                .filter(|&n| n != 0)
                .unwrap_or(*base_id);
            let species_dir =
                sprites_dir.join(format!("{:03}_{}", dex_num, sanitize_name(&base_name)));
            let forms_dir = species_dir.join("forms");

            for form in form_list {
                let form_dir = forms_dir.join(sanitize_name(&form.form));
                work.push((form, form_dir));
            }
        }

        let total = work.len() as u64 * 4;
        let pb = progress_bar(total, cli.quiet);

        let count = work.len();

        work.par_iter().try_for_each(|(form, form_dir)| {
            std::fs::create_dir_all(form_dir)
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
                    pb.inc(1);
                }
            }
            Ok::<(), anyhow::Error>(())
        })?;

        pb.finish_and_clear();
        Ok(count)
    }
}
