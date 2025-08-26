use anyhow::Result;
use clap::Parser;
use command_runner::SystemCommandRunner;
use log::warn;

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

fn run() -> Result<()> {
    let cli = Cli::parse();
    let lvl = cli.verbose.log_level_filter();
    env_logger::Builder::new()
        .filter_level(lvl)
        .format_target(false)
        .format_timestamp(None)
        .init();
    let runner = SystemCommandRunner;

    match cli.command {
        Commands::New {
            sandbox_name,
            no_git,
            standard,
        } => cmd_new(&runner, &sandbox_name, no_git, standard.unwrap_or_default())?,
        Commands::Add { dependency } => cmd_add(&runner, &dependency)?,
        Commands::Remove { dependency } => cmd_remove(&runner, &dependency)?,
        Commands::Build { build_type } => cmd_build(&runner, build_type.unwrap_or_default(), lvl)?,
        Commands::Run { build_type } => cmd_run(&runner, build_type.unwrap_or_default(), lvl)?,
        Commands::Clean => cmd_clean(&runner)?,
    }
    Ok(())
}

fn main() {
    match run() {
        Ok(_) => (),
        Err(e) => {
            warn!("{e}")
        }
    }
}
