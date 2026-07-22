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

    /// Glob pattern to filter which tilesets to extract.
    #[arg(long, value_name = "PATTERN")]
    pub tileset_filter: Option<String>,

    /// Glob pattern to filter which sprites to extract.
    #[arg(long, value_name = "PATTERN")]
    pub sprite_filter: Option<String>,
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

    let symbols = sym::load_symbols(&cli, &rom).with_context(|| "loading symbols")?;

    run_pack(&cli, &rom, &symbols, start)
}

fn run_pack(
    cli: &Cli,
    rom: &crate::Rom,
    symbols: &crate::SymbolTable,
    start: std::time::Instant,
) -> Result<()> {
    let q = cli.quiet;

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

    // Field effect sprites (surf blob, shadows, grass rustling, etc.)
    let fx_count = crate::sprite_pack::write_field_effects(rom, symbols, &cli.export, cli.quiet)
        .with_context(|| "writing field effects")?;
    if !q && fx_count > 0 {
        println!(
            "  {} Extracted {} field effects to {}",
            style("\u{2713}").green().bold(),
            style(fx_count).bold(),
            style(cli.export.join("field_effects").display()).bold(),
        );
    }

    // Pokemon battle sprites
    let sprites_dir = cli.export.join("pokemon");
    let sprites = extractor.sprites().with_context(|| "extracting sprites")?;
    let species_names = extractor
        .species_names()
        .with_context(|| "loading species names")?;
    let national_map = extractor
        .national_dex_map()
        .with_context(|| "loading national dex map")?;
    let count = write::output::write_sprites(&sprites, &national_map, &sprites_dir, cli)?;
    if !q {
        println!(
            "  {} Extracted {} pokemon sprites to {}",
            style("\u{2713}").green().bold(),
            style(count).bold(),
            style(sprites_dir.display()).bold(),
        );
    }
    let forms = extractor.forms().with_context(|| "extracting forms")?;
    if !forms.is_empty() {
        let form_count =
            write::output::write_forms(&forms, &species_names, &national_map, &sprites_dir, cli)?;
        if !q {
            println!(
                "  {} Extracted {} form sprites",
                style("\u{2713}").green().bold(),
                style(form_count).bold(),
            );
        }
    }

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
