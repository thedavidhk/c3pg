use anyhow::Result;
use clap::Parser;

use crate::cli::*;
use crate::commands::*;

mod cli;
mod cmake;
mod commands;
mod conan;
mod config;
mod dependency;
mod traits;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New {
            sandbox_name,
            no_git,
            standard,
        } => cmd_new(&sandbox_name, no_git, standard.unwrap_or_default())?,
        Commands::Add { dependency } => cmd_add(&dependency)?,
        Commands::Build { build_type } => cmd_build(build_type.unwrap_or_default())?,
        Commands::Run { build_type } => cmd_run(build_type.unwrap_or_default())?,
        Commands::Clean => cmd_clean()?,
    }
    Ok(())
}
