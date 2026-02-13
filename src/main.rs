use c3pg::{cmake, cli, command_runner, commands};

use anyhow::Result;
use clap::Parser;
use command_runner::SystemCommandRunner;
use log::warn;

use crate::cli::{Cli, Commands};
use crate::commands::{cmd_new, cmd_add, cmd_remove, cmd_build, cmd_run, cmd_test, cmd_clean};

fn build_type(release: bool) -> cmake::BuildType {
    if release {
        cmake::BuildType::Release
    } else {
        cmake::BuildType::Debug
    }
}

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
        Commands::Build { release } => cmd_build(&runner, build_type(release), lvl)?,
        Commands::Run { release } => cmd_run(&runner, build_type(release), lvl)?,
        Commands::Test(testargs) => cmd_test(&runner, testargs, lvl)?,
        Commands::Clean => cmd_clean(&runner)?,
    }
    Ok(())
}

fn main() {
    match run() {
        Ok(()) => (),
        Err(e) => {
            warn!("{e}");
            std::process::exit(1);
        }
    }
}
