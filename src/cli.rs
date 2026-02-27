use clap::{ArgAction, Parser, Subcommand};
use clap_derive::Args;
use clap_verbosity_flag::{InfoLevel, Verbosity};

use crate::cmake::{CppStandard, Sanitizers};

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

    #[command(flatten)]
    pub sanitizers: Sanitizers,

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

        /// Set the `CppStandard`
        #[arg(long)]
        standard: Option<CppStandard>,

        /// Print only the directory path (for use with `cd $(c3pg new myapp --print-path)`)
        #[arg(long)]
        print_path: bool,
    },
    /// Create a throwaway project in a temporary directory
    Scratch {
        /// Set the C++ standard
        #[arg(long)]
        standard: Option<CppStandard>,

        /// Print only the directory path (for use with `cd $(c3pg scratch --print-path)`)
        #[arg(long)]
        print_path: bool,
    },
    /// Initialize c3pg in the current directory
    Init {
        /// Do not initialize an empty git repository
        #[arg(long, action(ArgAction::SetFalse))]
        no_git: bool,

        /// Set the C++ standard
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
        /// Build in release mode (default: debug)
        #[arg(long, short)]
        release: bool,
        #[command(flatten)]
        sanitizers: Sanitizers,
    },
    /// Run the current sandbox project (build if necessary)
    Run {
        /// Build in release mode (default: debug)
        #[arg(long, short)]
        release: bool,
        /// Which executable target to run (required when multiple exist)
        #[arg(long)]
        target: Option<String>,
        #[command(flatten)]
        sanitizers: Sanitizers,
    },
    /// Format C/C++ source files with clang-format
    Fmt {
        /// Check formatting without modifying files (exit with error if unformatted)
        #[arg(long)]
        check: bool,
    },
    /// Lint C/C++ source files with clang-tidy
    Lint {
        /// Apply suggested fixes in-place
        #[arg(long)]
        fix: bool,
    },
    /// Testing
    Test(TestArgs),
    /// Remove artifacts that c3pg has generated in the past
    Clean,
}
