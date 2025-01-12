use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use conan::Conanfile;
use std::{process::Command, str::FromStr};

use crate::conan::Conan;

mod cmake;
mod conan;
mod dependency;

/// Top-level CLI parser.
#[derive(Parser, Debug)]
#[command(name = "cpp_sandbox")]
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

        /// Initialize an empty git repository in the sandbox
        #[arg(long, action)]
        git: bool,
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
        build_type: Option<String>,
    },
    /// Run the current sandbox project (build if necessary)
    Run {
        /// Build type (default: Debug)
        #[arg(long, short)]
        build_type: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { sandbox_name, git } => cmd_new(&sandbox_name, git)?,
        Commands::Add { dependency } => cmd_add(&dependency)?,
        Commands::Build { build_type } => cmd_build(build_type)?,
        Commands::Run { build_type } => cmd_run(build_type)?,
    }
    Ok(())
}

/// Create a new sandbox directory with a minimal setup (CMakeLists.txt, conanfile.py, main.cpp).
fn cmd_new(sandbox_name: &str, git: bool) -> Result<()> {
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
    let cmake_lists_content = format!(
        r#"cmake_minimum_required(VERSION 3.15)
project({} LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Include Conan-generated cmake files
# Typically: include(${{CMAKE_BINARY_DIR}}/conan_deps.cmake)

add_executable(sandbox main.cpp)
"#,
        sandbox_name
    );

    std::fs::write(
        format!("{}/CMakeLists.txt", sandbox_name),
        cmake_lists_content,
    )?;

    // 4. Write a minimal conanfile.py
    let conanfile = Conanfile::new();
    std::fs::write(
        format!("{}/conanfile.py", sandbox_name),
        conanfile.to_string().as_str(),
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

    // 1. Read and parse existing conanfile.py
    let conanfile_path = "conanfile.py";
    let contents = std::fs::read_to_string(conanfile_path)
        .with_context(|| format!("Could not read from {}", conanfile_path))?;
    let mut conanfile = Conanfile::from_str(contents.as_str())
        .with_context(|| format!("Could not parse conanfile {}", conanfile_path))?;

    // 2. Add dependency
    conanfile.add_requirement(dependency.clone());

    // 3. Write back the file
    std::fs::write(conanfile_path, conanfile.to_string())
        .with_context(|| format!("Could not write to {}", conanfile_path))?;

    println!("Added dependency '{}'", dependency);
    Ok(())
}

/// Build the current sandbox project.
/// Steps:
///   1. `conan install . --build=missing --output-folder=build`
///   2. `cmake -B build -DCMAKE_TOOLCHAIN_FILE=build/conan_toolchain.cmake -DCMAKE_BUILD_TYPE=Release`
///   3. `cmake --build build`
fn cmd_build(build_type: Option<String>) -> Result<()> {
    // Step 1: conan install
    Conan::new()?.install(".", "build")?;

    // Step 2: cmake configure
    let cmake_configure = Command::new("cmake")
        .args([
            "-B",
            "build",
            "-DCMAKE_TOOLCHAIN_FILE=build/conan_toolchain.cmake",
            format!(
                "-DCMAKE_BUILD_TYPE={}",
                build_type.unwrap_or("Debug".to_string())
            )
            .as_str(),
            "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON",
        ])
        .status()?;
    if !cmake_configure.success() {
        bail!("CMake configure failed");
    }

    // Step 3: cmake --build
    let cmake_build = Command::new("cmake").args(["--build", "build"]).status()?;
    if !cmake_build.success() {
        bail!("CMake build failed");
    }

    println!("Build successful!");
    Ok(())
}

/// Run the current sandbox project.
/// If the binary does not exist or is out of date, rebuild first, then run.
fn cmd_run(build_type: Option<String>) -> Result<()> {
    cmd_build(build_type)?;

    let binary_path = "./build/sandbox"; // or "./build/my_sandbox"
    if cfg!(target_os = "windows") {
        // On Windows, it would be something like "build\sandbox.exe"
        std::process::Command::new(format!("{}.exe", binary_path)).status()?;
    } else {
        std::process::Command::new(binary_path).status()?;
    }

    Ok(())
}
