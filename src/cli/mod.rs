//! CLI implementation: argument parsing, symbol lookup, output writing.

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

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

    /// Clear output directory before writing.
    #[arg(short, long)]
    pub clear: bool,

    /// Automatically clear output directory without prompting.
    #[arg(short = 'y', long)]
    pub yes: bool,

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
    log::info!("reading ROM: {}", cli.rom.display());
    let rom =
        crate::Rom::open(&cli.rom).with_context(|| format!("opening ROM {}", cli.rom.display()))?;
    log::info!(
        "identified: {} ({})",
        rom.game().name(),
        rom.game().short_name()
    );

    let (dump_sprites, dump_tilesets) = resolve_dump_flags(&cli);
    let symbols =
        sym::load_or_download(&cli, rom.game(), dump_sprites).with_context(|| "loading symbols")?;

    if cli.clear {
        clear_dir(&cli.export)?;
    } else if !cli.yes && has_contents(&cli.export) && !prompt_clear(&cli.export)? {
        log::info!("skipping clear of {}", cli.export.display());
    }

    let extractor = crate::Extractor::new(&rom, &symbols);
    write::run(&cli, &extractor, dump_sprites, dump_tilesets)
}

fn resolve_dump_flags(cli: &Cli) -> (bool, bool) {
    let any = cli.sprites || cli.tilesets;
    if any {
        (cli.sprites, cli.tilesets)
    } else {
        (true, true)
    }
}

fn has_contents(path: &std::path::Path) -> bool {
    path.is_dir() && std::fs::read_dir(path).is_ok_and(|mut d| d.next().is_some())
}

fn prompt_clear(path: &std::path::Path) -> Result<bool> {
    print!(
        "Output directory {} contains files. Clear before writing? [y/N] ",
        path.display()
    );
    std::io::stdout().flush().ok();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let lower = line.trim().to_ascii_lowercase();
    Ok(lower == "y" || lower == "yes")
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
