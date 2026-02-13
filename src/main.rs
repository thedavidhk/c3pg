use c3pg::{cmake, cli, command_runner, commands, format, ui};

use anyhow::Result;
use clap::Parser;
use command_runner::SystemCommandRunner;

use crate::cli::{Cli, Commands};
use crate::commands::{cmd_new, cmd_init, cmd_add, cmd_remove, cmd_build, cmd_run, cmd_test, cmd_clean, load_config};

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

    // Only activate the log framework at debug/trace verbosity.
    // Normal user output goes through the `ui` module instead.
    if lvl >= log::LevelFilter::Debug {
        env_logger::Builder::new()
            .filter_level(lvl)
            .format_target(false)
            .format_timestamp(None)
            .init();
    }

    let runner = SystemCommandRunner;

    match cli.command {
        Commands::New {
            sandbox_name,
            no_git,
            standard,
        } => cmd_new(&runner, &sandbox_name, no_git, standard.unwrap_or_default())?,
        Commands::Init {
            no_git,
            standard,
        } => cmd_init(&runner, no_git, standard.unwrap_or_default())?,
        Commands::Add { dependency } => cmd_add(&runner, &dependency)?,
        Commands::Remove { dependency } => cmd_remove(&runner, &dependency)?,
        Commands::Build { release, sanitizers } => {
            cmd_build(&runner, build_type(release), lvl, &sanitizers)?;
        }
        Commands::Run { release, sanitizers } => {
            cmd_run(&runner, build_type(release), lvl, &sanitizers)?;
        }
        Commands::Fmt { check } => {
            let config = load_config(&runner)?;
            format::cmd_fmt(&runner, lvl, &config.testing.dir, check)?;
        }
        Commands::Lint { fix } => {
            let config = load_config(&runner)?;
            format::cmd_lint(
                &runner,
                lvl,
                &config.testing.dir,
                &config.project.cache_dir,
                fix,
            )?;
        }
        Commands::Test(testargs) => cmd_test(&runner, testargs, lvl)?,
        Commands::Clean => cmd_clean(&runner)?,
    }
    Ok(())
}

fn main() {
    match run() {
        Ok(()) => (),
        Err(e) => {
            ui::error(&format!("{e}"));
            for cause in e.chain().skip(1) {
                eprintln!("  caused by: {cause}");
            }
            std::process::exit(1);
        }
    }
}
