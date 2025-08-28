use clap::{ArgAction, Parser, Subcommand};
use clap_derive::Args;
use clap_verbosity_flag::{InfoLevel, Verbosity};

use crate::cmake::{BuildType, CppStandard};

/// c3pg: Create, manage, and run C++ project sandboxes
#[derive(Parser, Debug)]
#[command(name = "c3pg")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    #[command(flatten)]
    pub verbose: Verbosity<InfoLevel>,
}

#[derive(Subcommand, Debug)]
pub enum TestOnlySubcmds {
    /// Add a new test by name
    Add { name: String },
}

#[derive(Args, Debug)]
#[command(
    // Make the "run-like" args conflict with subcommands,
    // so using a subcommand hides/negates them.
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true,
    // Optional: explain the default in help/usage
    about = "Testing (default: runs tests if no subcommand is given)"
)]
pub struct TestArgs {
    /// Expression to match test cases to run
    #[arg(short, long)]
    pub filter: Option<String>,

    /// Number of parallel jobs
    #[arg(short, long)]
    pub jobs: Option<u8>,

    /// Other test-related subcommands
    #[command(subcommand)]
    pub command: Option<TestOnlySubcmds>,
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
    /// Remove a Conan dependency from the current sandbox (in the current working directory)
    Remove {
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
    /// Testing
    Test(TestArgs),
    /// Remove artifacts that c3pg has generated in the past
    Clean,
}
