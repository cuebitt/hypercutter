//! `hypercutter` command-line entry point.

#![cfg(not(target_arch = "wasm32"))]

use anyhow::Result;
use clap::Parser;

use hypercutter::cli;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or(if cli.verbose { "debug" } else { "info" }),
    )
    .init();
    cli::run(cli)
}
