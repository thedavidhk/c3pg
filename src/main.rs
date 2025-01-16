use anyhow::{anyhow, Result};
use clap::{ArgAction, Parser, Subcommand};
use cmake::{BuildType, CMake, CppStandard};
use conan::{Conan, Conanfile};
use std::process::Command;

mod cmake;
mod conan;
mod dependency;

/// cpppg: Create, manage, and run C++ project sandboxes
#[derive(Parser, Debug)]
#[command(name = "cpppg")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// List of subcommands.
#[derive(Subcommand, Debug)]
enum Commands {
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
}

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
    }
    Ok(())
}

/// Create a new sandbox directory with a minimal setup (CMakeLists.txt, conanfile.py, main.cpp).
fn cmd_new(sandbox_name: &str, git: bool, standard: CppStandard) -> Result<()> {
    // 1. Create the sandbox directory
    std::fs::create_dir(sandbox_name)?;

    // 2. Write main.cpp
    let main_cpp_content = r#"#include <iostream>

int main() {
    std::cout << "Hello from C++ sandbox!" << std::endl;
    return 0;
}
"#;
    std::fs::write(format!("{}/main.cpp", sandbox_name), main_cpp_content)?;

    // 3. Write a minimal CMakeLists.txt
    let cmake = CMake::new(sandbox_name.to_string(), standard, true);
    std::fs::write(
        format!("{}/CMakeLists.txt", sandbox_name),
        cmake.to_string(),
    )?;

    // 4. Write a minimal conanfile.py
    let conanfile = Conanfile::new();
    std::fs::write(
        format!("{}/conanfile.py", sandbox_name),
        conanfile.to_string(),
    )?;

    if git {
        // Write a .gitignore
        let gitignore_content = r#"build/
CMakeLists.txt
CMakeUserPresets.json
conanfile.py
"#;
        std::fs::write(format!("{}/.gitignore", sandbox_name), gitignore_content)?;

        // Initialize empty git repo
        Command::new("git").args(["init", sandbox_name]).status()?;
    }

    println!("Created new sandbox: {}", sandbox_name);
    Ok(())
}

/// Add a Conan dependency to conanfile.py in the current directory.
fn cmd_add(expr: &str) -> Result<()> {
    // Find dependency
    let dependency = Conan::new()?
        .get_latest_matching_dependency(expr)
        .ok_or(anyhow!("Could not find dependency {} in remotes", expr))?;

    // 1. Read and parse existing conanfile.py and CMakeLists.txt
    let conanfile_path = "conanfile.py";
    let mut conanfile = Conanfile::from_file(conanfile_path)?;
    let cmake_path = "CMakeLists.txt";
    let mut cmake = CMake::from_file(cmake_path)?;

    // 2. Add dependency
    conanfile.add_dependency(dependency.clone());
    cmake.add_dependency(&dependency);

    // 3. Write back the file
    conanfile.to_file(conanfile_path)?;
    cmake.to_file(cmake_path)?;

    println!("Added dependency '{}'", dependency);
    Ok(())
}

/// Build the current sandbox project.
fn cmd_build(build_type: BuildType) -> Result<()> {
    Conan::new()?.install(".", "build")?;
    CMake::build(build_type)?;

    println!("Build successful!");
    Ok(())
}

/// Run the current sandbox project.
/// If the binary does not exist or is out of date, rebuild first, then run.
fn cmd_run(build_type: BuildType) -> Result<()> {
    cmd_build(build_type)?;

    let binary_path = "./build/sandbox"; // or "./build/my_sandbox"
    if cfg!(target_os = "windows") {
        std::process::Command::new(format!("{}.exe", binary_path)).status()?;
    } else {
        std::process::Command::new(binary_path).status()?;
    }

    Ok(())
}
