use anyhow::{anyhow, Context, Result};
use clap::{ArgAction, Parser, Subcommand};
use cmake::{BuildType, CMake, CppStandard};
use conan::Conan;
use config::Config;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
};

use crate::traits::{FromFile, ToFile};

mod cmake;
mod conan;
mod config;
mod dependency;
mod traits;

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
    /// Remove artifacts that CPPPG has generated in the past
    Clean,
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
        Commands::Clean => cmd_clean()?,
    }
    Ok(())
}

/// Create a new sandbox directory with a minimal setup (CMakeLists.txt, conanfile.py, main.cpp).
fn cmd_new(sandbox_name: &str, git: bool, standard: CppStandard) -> Result<()> {
    // 1. Create the sandbox directory
    let project_path = PathBuf::from(sandbox_name);
    let src_dir = project_path.join("src");
    fs::create_dir_all(&src_dir).context("Could not create src_dir")?;
    let build_dir = project_path.join("build");
    fs::create_dir_all(&build_dir).context("Could not create build dir")?;

    // 2. Write main.cpp
    let main_cpp_content = r#"#include <iostream>

int main() {
    std::cout << "Hello from C++ sandbox!" << std::endl;
    return 0;
}
"#;
    fs::write(src_dir.join("main.cpp"), main_cpp_content).context("Could not write to main.cpp")?;

    // 3. Write minimal config files
    let mut config = Config::default();
    config.project.name = sandbox_name.to_string();
    config.cmake.standard = standard;
    CMake::from_config(&config).to_file(build_dir.join("CMakeLists.txt"))?;
    Conan::from_config(&config)?.to_file(build_dir.join("conanfile.py"))?;
    config.to_file(project_path.join("cpppg.toml"))?;

    if git {
        // Write a .gitignore
        let gitignore_content = r#"build/
CMakeLists.txt
CMakeUserPresets.json
conanfile.py
"#;
        fs::write(format!("{}/.gitignore", sandbox_name), gitignore_content)
            .context("Could not create gitignore")?;

        // Initialize empty git repo
        Command::new("git")
            .args(["init", "-b", "main", sandbox_name])
            .stdout(Stdio::null())
            .status()?;
    }

    println!("Created new sandbox: {}", sandbox_name);
    Ok(())
}

/// Add a Conan dependency to conanfile.py in the current directory.
fn cmd_add(expr: &str) -> Result<()> {
    let build_dir = PathBuf::from_str("build")?;
    let config_file = PathBuf::from_str("cpppg.toml")?;
    let mut config = regenerate_config(&build_dir, &config_file)?;

    // Find dependency
    let dependency = Conan::from_config(&config)?
        .get_latest_matching_dependency(expr)
        .ok_or(anyhow!("Could not find dependency {} in remotes", expr))?;

    // 1. Read and parse existing conanfile.py and CMakeLists.txt
    let conanfile_path = "build/conanfile.py";
    let cmake_path = "build/CMakeLists.txt";

    // 2. Add dependency
    config.project.add_dependency(dependency.clone());
    Conan::from_config(&config)?.to_file(conanfile_path)?;
    CMake::from_config(&config).to_file(cmake_path)?;
    config.to_file("cpppg.toml")?;

    println!("Added dependency '{}'", dependency);
    Ok(())
}

fn regenerate_config(cache_dir: &Path, config_file: &Path) -> Result<Config> {
    if !cache_dir.is_dir() {
        fs::create_dir_all(cache_dir)?;
    }
    let conanfile_path = cache_dir.join("conanfile.py");
    let cmake_path = cache_dir.join("CMakeLists.txt");
    let config = Config::from_file(config_file).context("Failed to load cpppg.toml")?;

    // Ensure config files are regenerated if missing or outdated
    let config_modified = fs::metadata(config_file)?.modified()?;
    if conanfile_path.exists() {
        let conanfile_modified = fs::metadata(&conanfile_path)?.modified()?;
        if conanfile_modified < config_modified {
            Conan::from_config(&config)?.to_file(&conanfile_path)?;
        }
    } else {
        Conan::from_config(&config)?.to_file(&conanfile_path)?;
    }

    if cmake_path.exists() {
        let cmake_modified = fs::metadata(&cmake_path)?.modified()?;
        if cmake_modified < config_modified {
            CMake::from_config(&config).to_file(&cmake_path)?;
        }
    } else {
        CMake::from_config(&config).to_file(&cmake_path)?;
    }

    Ok(config)
}

/// Build the current sandbox project.
fn cmd_build(build_type: BuildType) -> Result<()> {
    let config_path = PathBuf::from("cpppg.toml");
    let build_dir = PathBuf::from("build");

    let config = regenerate_config(&build_dir, &config_path)?;

    Conan::from_config(&config)?.install("build", "build")?;
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

fn cmd_clean() -> Result<()> {
    let cache_dir = PathBuf::from_str("build")?;
    if cache_dir.is_dir() {
        fs::remove_dir_all(cache_dir)?;
    }

    println!("Removed all build artifacts");

    Ok(())
}
