//! Symbol-file discovery: local path, cache, or download.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use console::style;

use crate::{Extractor, Rom, SymbolTable};

use super::Cli;

/// Load symbols from a local path, cached download, or fetch from jsDelivr.
///
/// When `--sym-file` is provided, that file is used as-is. Otherwise the
/// default `.sym` is downloaded (or read from cache). If the downloaded
/// symbols fail validation against the ROM — a common problem when the
/// user's ROM is a different revision than the default `.sym` — each
/// revision-specific file is tried until one matches.
pub(crate) fn load_or_download(cli: &Cli, rom: &Rom) -> Result<SymbolTable> {
    if let Some(sym) = &cli.sym_file {
        return SymbolTable::from_path(sym).with_context(|| format!("parsing {}", sym.display()));
    }

    let q = cli.quiet;
    let game = rom.game();
    let cache_dir = cache_dir().context("resolving cache directory")?;
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("creating {}", cache_dir.display()))?;

    // 1. Try the default .sym file.
    let default_name = game.default_sym_file();
    let default_cached = cache_dir.join(default_name);
    if !default_cached.exists() {
        let url = game.sym_url();
        if !q {
            println!(
                "  {} Downloading symbols from {}",
                style("\u{2193}").cyan().bold(),
                style(&url).dim(),
            );
        }
        download(&url, &default_cached)?;
    } else {
        log::debug!("using cached symbols at {}", default_cached.display());
    }

    let symbols = SymbolTable::from_path(&default_cached)
        .with_context(|| format!("parsing {}", default_cached.display()))?;

    if validate(rom, &symbols) {
        if !q {
            println!(
                "  {} Symbol file matches ROM",
                style("\u{2713}").green().bold(),
            );
        }
        return Ok(symbols);
    }

    if !q {
        println!(
            "  {} Default symbol file ({}) does not match ROM \u{2014} trying revisions",
            style("\u{26a0}").yellow().bold(),
            style(default_name).dim(),
        );
    }

    // 2. Try revision-specific .sym files.
    for rev_name in game.revision_sym_files() {
        let rev_cached = cache_dir.join(rev_name);
        if !rev_cached.exists() {
            let url = game.sym_url_for_file(rev_name);
            if !q {
                println!(
                    "  {} Downloading revision symbols from {}",
                    style("\u{2193}").cyan().bold(),
                    style(&url).dim(),
                );
            }
            if let Err(e) = download(&url, &rev_cached) {
                if !q {
                    println!(
                        "  {} Failed to download {}: {}",
                        style("\u{26a0}").yellow().bold(),
                        style(rev_name).dim(),
                        style(e).red(),
                    );
                }
                continue;
            }
        } else {
            log::debug!("using cached revision symbols at {}", rev_cached.display());
        }

        match SymbolTable::from_path(&rev_cached) {
            Ok(rev_symbols) => {
                if validate(rom, &rev_symbols) {
                    if !q {
                        println!(
                            "  {} Revision symbol file {} matches ROM",
                            style("\u{2713}").green().bold(),
                            style(rev_name).dim(),
                        );
                    }
                    return Ok(rev_symbols);
                }
                log::debug!("{rev_name} does not match ROM");
            }
            Err(e) => {
                if !q {
                    println!(
                        "  {} Failed to parse {}: {}",
                        style("\u{26a0}").yellow().bold(),
                        style(rev_name).dim(),
                        style(e).red(),
                    );
                }
            }
        }
    }

    // 3. Fall back to the default even though it doesn't match.
    if !q {
        println!(
            "  {} No matching revision found \u{2014} using default ({})",
            style("\u{26a0}").yellow().bold(),
            style(default_name).dim(),
        );
    }
    Ok(symbols)
}

/// Quick heuristic validation: check that at least one tileset symbol
/// produces in-range pointers in its `TilesetHeader`.
fn validate(rom: &Rom, symbols: &SymbolTable) -> bool {
    let extractor = Extractor::new(rom, symbols);
    extractor.validate()
}

fn download(url: &str, dest: &Path) -> Result<()> {
    let mut response = ureq::get(url)
        .call()
        .with_context(|| format!("GET {url}"))?;
    let mut body = String::new();
    use std::io::Read;
    response
        .body_mut()
        .as_reader()
        .read_to_string(&mut body)
        .with_context(|| format!("reading body from {url}"))?;
    std::fs::write(dest, body).with_context(|| format!("writing {}", dest.display()))?;
    Ok(())
}

fn cache_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("HYPERCUTTER_CACHE_DIR") {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    dirs::cache_dir().map(|p| p.join("hypercutter"))
}
