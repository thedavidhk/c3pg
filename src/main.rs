use anyhow::Result;
use clap::Parser;
use command_runner::SystemCommandRunner;

use crate::cli::*;
use crate::commands::*;

mod cli;
mod cmake;
mod command_runner;
mod commands;
mod conan;
mod config;
mod dependency;
mod traits;

#[cfg(test)]
mod test_utils;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let runner = SystemCommandRunner;

    match cli.command {
        Commands::New {
            sandbox_name,
            no_git,
            standard,
        } => cmd_new(&runner, &sandbox_name, no_git, standard.unwrap_or_default())?,
        Commands::Add { dependency } => cmd_add(&runner, &dependency)?,
        Commands::Build { build_type } => cmd_build(&runner, build_type.unwrap_or_default())?,
        Commands::Run { build_type } => cmd_run(&runner, build_type.unwrap_or_default())?,
        Commands::Clean => cmd_clean(&runner)?,
    }
    Ok(())
}
