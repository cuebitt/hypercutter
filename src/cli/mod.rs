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
    name = "hypercutter",
    version,
    about = "Extract data from GBA Pokemon ROMs",
    long_about = None
)]
pub struct Cli {
    /// Path to a .gba ROM file.
    pub rom: PathBuf,

    /// Game to use for symbol file download. Supported:
    /// emerald, firered, leafgreen, ruby, sapphire.
    #[arg(short, long)]
    pub game: Option<String>,

    /// Path to a .sym file (auto-downloaded if not provided).
    #[arg(long = "sym-file", value_name = "SYM")]
    pub sym_file: Option<PathBuf>,

    /// Directory to export PNGs.
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
}

/// Run the CLI with the given arguments.
///
/// # Errors
///
/// Returns an error if the ROM cannot be opened, symbols cannot be loaded,
/// or any PNG export fails.
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

    let (dump_sprites, dump_tilesets) = resolve_dump_flags(&cli);
    let symbols = sym::load_or_download(&cli, &rom).with_context(|| "loading symbols")?;

    if cli.list {
        return list_contents(&cli, &rom, &symbols, dump_sprites, dump_tilesets);
    }

    if cli.clear {
        clear_dir(&cli.export)?;
    } else if !cli.yes
        && !cli.overwrite
        && has_contents(&cli.export)
        && !q
        && !prompt_clear(&cli.export)?
    {
        println!(
            "  {} Skipping clear of {}",
            style("\u{2192}").cyan().bold(),
            style(cli.export.display()).bold(),
        );
    }

    let extractor = crate::Extractor::new(&rom, &symbols);
    let summary = write::run(&cli, &extractor, dump_sprites, dump_tilesets)?;

    let elapsed = start.elapsed();
    if !q {
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
        println!(
            "  {} Done in {}",
            style("\u{2192}").cyan().bold(),
            style(format!("{:.2?}", elapsed)).bold(),
        );
    }

    Ok(())
}

fn resolve_dump_flags(cli: &Cli) -> (bool, bool) {
    let any = cli.sprites || cli.tilesets;
    if any {
        (cli.sprites, cli.tilesets)
    } else {
        (true, true)
    }
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
