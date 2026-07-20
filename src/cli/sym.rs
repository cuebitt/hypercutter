//! Symbol-file loading: bundled or user-supplied TOML/.sym.

use anyhow::{Context, Result};

use crate::{Rom, SymbolTable};

use super::Cli;

/// Load symbols from a bundled table (default) or a user-supplied
/// `--sym-file` (TOML, falling back to legacy `.sym`).
pub(crate) fn load_symbols(cli: &Cli, rom: &Rom) -> Result<SymbolTable> {
    let Some(path) = &cli.sym_file else {
        return SymbolTable::resolve_for_rom(rom).with_context(|| "resolving bundled symbol table");
    };

    // User supplied a file.  Try TOML first, then legacy .sym.
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

    // Try TOML.
    if let Ok(table) = SymbolTable::from_toml(&text) {
        return Ok(table);
    }

    // Fall back to legacy .sym.
    SymbolTable::from_text(&text)
        .with_context(|| format!("parsing {} (neither valid TOML nor .sym)", path.display()))
}
