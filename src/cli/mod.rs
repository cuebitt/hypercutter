//! CLI implementation: argument parsing, symbol lookup, output writing.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use console::style;
use dialoguer::Confirm;

pub mod sym;
pub mod write;

/// Command-line arguments.
#[derive(Parser, Debug)]
#[command(
    name = "hc",
    version,
    about = "Extract data from GBA Pokemon ROMs",
    long_about = None
)]
pub struct Cli {
    /// Path to a .gba ROM file.
    pub rom: PathBuf,

    /// Path to a symbol file (TOML, or legacy .sym).
    /// By default, bundled symbol tables are used.
    #[arg(long = "sym-file", value_name = "SYM")]
    pub sym_file: Option<PathBuf>,

    /// Directory to export data to.
    #[arg(short, long, default_value = "out")]
    pub export: PathBuf,

    /// Verbose output.
    #[arg(short, long)]
    pub verbose: bool,

    /// Suppress all non-error output.
    #[arg(short, long)]
    pub quiet: bool,

    /// Clear output directory before writing.
    #[arg(short, long)]
    pub clear: bool,

    /// Automatically clear output directory without prompting.
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Write over existing files without prompting.
    #[arg(long)]
    pub overwrite: bool,

    /// List available tilesets or sprites without extracting.
    #[arg(long)]
    pub list: bool,

    /// Glob pattern to filter which tilesets to extract.
    #[arg(long, value_name = "PATTERN")]
    pub tileset_filter: Option<String>,

    /// Glob pattern to filter which sprites to extract.
    #[arg(long, value_name = "PATTERN")]
    pub sprite_filter: Option<String>,

    /// Dump Pokemon battle sprites (front/back, normal + shiny).
    #[arg(long)]
    pub sprites: bool,

    /// Render and export tileset PNGs.
    #[arg(long)]
    pub tilesets: bool,

    /// Output sprites as a spritesheet instead of individual PNGs.
    #[arg(long)]
    pub spritesheet: bool,

    /// Columns in the spritesheet.
    #[arg(long, default_value_t = 8)]
    pub spritesheet_columns: usize,

    /// Use the flat export format (tileset PNGs + battle sprite PNGs).
    /// By default, hc exports a sprite pack (field/overworld sprites in a
    /// facing-frames grid layout with a manifest.json).
    /// Note: --sprites and --tilesets imply --flat if not already set.
    #[arg(long)]
    pub flat: bool,
}

/// Run the CLI with the given arguments.
///
/// # Errors
///
/// Returns an error if the ROM cannot be opened, symbols cannot be loaded,
/// or any export fails.
pub fn run(cli: Cli) -> Result<()> {
    let start = std::time::Instant::now();
    let q = cli.quiet;

    if !q {
        println!(
            "  {} Reading ROM: {}",
            style("\u{2192}").cyan().bold(),
            style(cli.rom.display()).bold(),
        );
    }
    let rom =
        crate::Rom::open(&cli.rom).with_context(|| format!("opening ROM {}", cli.rom.display()))?;
    if !q {
        println!(
            "  {} Identified: {} ({})",
            style("\u{2192}").cyan().bold(),
            style(rom.game().name()).bold(),
            rom.game().short_name(),
        );
    }

    let use_flat = cli.flat || cli.sprites || cli.tilesets;
    let symbols = sym::load_symbols(&cli, &rom).with_context(|| "loading symbols")?;

    if use_flat {
        run_flat(&cli, &rom, &symbols, start)?;
    } else {
        run_pack(&cli, &rom, &symbols, start)?;
    }

    Ok(())
}

fn run_flat(
    cli: &Cli,
    rom: &crate::Rom,
    symbols: &crate::SymbolTable,
    start: std::time::Instant,
) -> Result<()> {
    let q = cli.quiet;
    let (dump_sprites, dump_tilesets) = resolve_flat_flags(cli);
    let dump_any = dump_sprites || dump_tilesets;

    if cli.list {
        return list_contents(cli, rom, symbols, dump_sprites, dump_tilesets);
    }

    ensure_clear(cli)?;

    let extractor = crate::Extractor::new(rom, symbols);
    let summary = write::run(cli, &extractor, dump_sprites, dump_tilesets)?;
    let elapsed = start.elapsed();

    if !q && dump_any {
        let mut parts = Vec::new();
        if summary.tilesets > 0 {
            parts.push(format!("{} tilesets", style(summary.tilesets).bold()));
        }
        if summary.sprites > 0 {
            parts.push(format!("{} sprites", style(summary.sprites).bold()));
        }
        if summary.forms > 0 {
            parts.push(format!("{} forms", style(summary.forms).bold()));
        }
        if !parts.is_empty() {
            println!(
                "  {} {} to {}",
                style("\u{2713}").green().bold(),
                parts.join(", "),
                style(cli.export.display()).bold(),
            );
        }
    }

    if !q {
        println!(
            "  {} Done in {}",
            style("\u{2192}").cyan().bold(),
            style(format!("{:.2?}", elapsed)).bold(),
        );
    }

    Ok(())
}

fn run_pack(
    cli: &Cli,
    rom: &crate::Rom,
    symbols: &crate::SymbolTable,
    start: std::time::Instant,
) -> Result<()> {
    let q = cli.quiet;

    if cli.list {
        anyhow::bail!("--list is only supported with --flat (sprite pack mode has no tileset/battle-sprite listing)");
    }

    ensure_clear(cli)?;

    if !q {
        println!(
            "  {} Extracting tilesets...",
            style("\u{2192}").cyan().bold(),
        );
    }
    let extractor = crate::Extractor::new(rom, symbols);
    let metatiles = extractor
        .metatiles()
        .with_context(|| "extracting metatiles")?;
    let tilesets_dir = cli.export.join("tilesets");
    write::output::write_tileset_pngs(&metatiles, &tilesets_dir, extractor.rom().game(), cli)?;
    if !q {
        println!(
            "  {} Tilesets written to {}",
            style("\u{2713}").green().bold(),
            style(tilesets_dir.display()).bold(),
        );
    }

    crate::sprite_pack::write_pack(rom, symbols, &cli.export, cli.quiet)
        .with_context(|| "writing sprite pack")?;

    if !q {
        let elapsed = start.elapsed();
        println!(
            "  {} Done in {}",
            style("\u{2192}").cyan().bold(),
            style(format!("{:.2?}", elapsed)).bold(),
        );
    }

    Ok(())
}

fn resolve_flat_flags(cli: &Cli) -> (bool, bool) {
    let any = cli.sprites || cli.tilesets;
    if any {
        (cli.sprites, cli.tilesets)
    } else {
        (true, true)
    }
}

fn ensure_clear(cli: &Cli) -> Result<()> {
    if cli.clear {
        clear_dir(&cli.export)?;
    } else if !cli.yes
        && !cli.overwrite
        && has_contents(&cli.export)
        && !cli.quiet
        && !prompt_clear(&cli.export)?
    {
        println!(
            "  {} Skipping clear of {}",
            style("\u{2192}").cyan().bold(),
            style(cli.export.display()).bold(),
        );
    }
    Ok(())
}

pub(crate) struct Summary {
    pub tilesets: usize,
    pub sprites: usize,
    pub forms: usize,
}

fn list_contents(
    cli: &Cli,
    rom: &crate::Rom,
    symbols: &crate::SymbolTable,
    dump_sprites: bool,
    dump_tilesets: bool,
) -> Result<()> {
    let extractor = crate::Extractor::new(rom, symbols);

    if dump_tilesets {
        let metatiles = extractor
            .metatiles()
            .with_context(|| "extracting metatiles")?;
        let exclude = game_exclude(rom.game());
        let filter = cli
            .tileset_filter
            .as_deref()
            .map(glob::Pattern::new)
            .transpose()
            .with_context(|| "invalid tileset filter pattern")?;

        println!("{}", style("Tilesets:").bold());
        for name in metatiles.names() {
            if exclude.contains(&name) {
                continue;
            }
            if let Some(ref pat) = filter {
                if !pat.matches(name) {
                    continue;
                }
            }
            println!("  {name}");
        }
    }

    if dump_sprites {
        let species_names = extractor
            .species_names()
            .with_context(|| "loading species names")?;
        let filter = cli
            .sprite_filter
            .as_deref()
            .map(glob::Pattern::new)
            .transpose()
            .with_context(|| "invalid sprite filter pattern")?;

        println!("{}", style("Sprites:").bold());
        for (i, name) in species_names.iter().enumerate() {
            if name.is_empty() || name == "?" {
                continue;
            }
            if let Some(ref pat) = filter {
                if !pat.matches(name) {
                    continue;
                }
            }
            println!("  {:03}: {name}", i);
        }
    }

    Ok(())
}

pub(crate) fn game_exclude(game: crate::Game) -> &'static [&'static str] {
    match game {
        crate::Game::FireRed | crate::Game::LeafGreen => &["HoennBuilding"],
        _ => &[],
    }
}

fn has_contents(path: &std::path::Path) -> bool {
    path.is_dir() && std::fs::read_dir(path).is_ok_and(|mut d| d.next().is_some())
}

fn prompt_clear(path: &std::path::Path) -> Result<bool> {
    let prompt = format!(
        "Output directory {} contains files. Clear before writing?",
        path.display()
    );
    let answer = Confirm::new()
        .with_prompt(&prompt)
        .default(false)
        .interact()
        .with_context(|| "reading user input")?;
    Ok(answer)
}

fn clear_dir(path: &std::path::Path) -> Result<()> {
    if path.is_file() {
        std::fs::remove_file(path).with_context(|| format!("removing {}", path.display()))?;
        return Ok(());
    }
    if !path.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(path).with_context(|| format!("reading {}", path.display()))? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            std::fs::remove_dir_all(&entry_path)
                .with_context(|| format!("removing {}", entry_path.display()))?;
        } else {
            std::fs::remove_file(&entry_path)
                .with_context(|| format!("removing {}", entry_path.display()))?;
        }
    }
    Ok(())
}
