use clap::{ArgAction, Parser, Subcommand};

use crate::cmake::{BuildType, CppStandard};

/// cpppg: Create, manage, and run C++ project sandboxes
#[derive(Parser, Debug)]
#[command(name = "cpppg")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// List of subcommands.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Create a new sandbox directory with the given name
    New {
        /// The name of the new sandbox directory
        sandbox_name: String,

        /// Do not initialize an empty git repository in the sandbox
        #[arg(long, action(ArgAction::SetFalse))]
        no_git: bool,

        /// Set the CppStandard
        #[arg(long)]
        standard: Option<CppStandard>,
    },
    /// Add a Conan dependency to the current sandbox (in the current working directory)
    Add {
        /// Name of the Conan dependency (e.g. fmt/10.1.0)
        dependency: String,
    },
    /// Build the current sandbox project (in the current working directory)
    Build {
        /// Build type (default: Debug)
        #[arg(long, short)]
        build_type: Option<BuildType>,
    },
    /// Run the current sandbox project (build if necessary)
    Run {
        /// Build type (default: Debug)
        #[arg(long, short)]
        build_type: Option<BuildType>,
    },
    /// Remove artifacts that CPPPG has generated in the past
    Clean,
}
