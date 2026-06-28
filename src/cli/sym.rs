//! Symbol-file discovery: local path, cache, or download.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::{Game, SymbolTable};

use super::Cli;

/// Load symbols from a local path, cached download, or fetch from jsDelivr.
pub(crate) fn load_or_download(cli: &Cli, game: Game) -> Result<SymbolTable> {
    if let Some(sym) = &cli.sym_file {
        return SymbolTable::from_path(sym).with_context(|| format!("parsing {}", sym.display()));
    }
    let cache_dir = cache_dir().context("resolving cache directory")?;
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("creating {}", cache_dir.display()))?;
    let sym_name = game.default_sym_file();
    let cached = cache_dir.join(sym_name);
    if !cached.exists() {
        let url = game.sym_url();
        log::info!("downloading symbols from {url}");
        download(&url, &cached)?;
    } else {
        log::debug!("using cached symbols at {}", cached.display());
    }
    SymbolTable::from_path(&cached).with_context(|| format!("parsing {}", cached.display()))
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
