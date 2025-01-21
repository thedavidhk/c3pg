use anyhow::{anyhow, Context, Result};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

use crate::cmake::{BuildType, CMake, CppStandard};
use crate::conan::Conan;
use crate::config::Config;
use crate::traits::{FromFile, ToFile};

/// Create a new sandbox directory with a minimal setup (CMakeLists.txt, conanfile.py, main.cpp).
pub fn cmd_new(sandbox_name: &str, git: bool, standard: CppStandard) -> Result<()> {
    // 1. Create the sandbox directory
    let project_path = PathBuf::from(sandbox_name);
    let src_dir = project_path.join("src");
    fs::create_dir_all(&src_dir).context("Could not create src_dir")?;
    let mut config = Config::default();
    let cache_dir = project_path.join(&config.project.cache_dir);
    fs::create_dir_all(&cache_dir).context("Could not create build dir")?;

    // 2. Write main.cpp
    let main_cpp_content = r#"#include <iostream>

int main() {
    std::cout << "Hello from C++ sandbox!" << std::endl;
    return 0;
}
"#;
    fs::write(src_dir.join("main.cpp"), main_cpp_content).context("Could not write to main.cpp")?;

    // 3. Write minimal config files
    config.project.name = sandbox_name.to_string();
    config.cmake.standard = standard;
    CMake::from_config(&config).to_file(cache_dir.join("CMakeLists.txt"))?;
    Conan::from_config(&config)?.to_file(cache_dir.join("conanfile.py"))?;
    config.to_file(project_path.join("cpppg.toml"))?;

    if git {
        // Write a .gitignore
        let gitignore_content = config.project.cache_dir.clone();
        fs::write(project_path.join(".gitignore"), gitignore_content)
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
pub fn cmd_add(expr: &str) -> Result<()> {
    let mut config = build_config()?;

    // Find dependency
    let dependency = Conan::from_config(&config)?
        .get_latest_matching_dependency(expr)
        .ok_or(anyhow!("Could not find dependency {} in remotes", expr))?;

    // 1. Read and parse existing conanfile.py and CMakeLists.txt
    let cache_dir = PathBuf::from(&config.project.cache_dir);
    let conanfile_path = cache_dir.join("conanfile.py");
    let cmake_path = cache_dir.join("CMakeLists.txt");

    // 2. Add dependency
    config.project.add_dependency(dependency.clone());
    Conan::from_config(&config)?.to_file(conanfile_path)?;
    CMake::from_config(&config).to_file(cmake_path)?;
    config.to_file("cpppg.toml")?;

    println!("Added dependency '{}'", dependency);
    Ok(())
}

/// Build the current sandbox project.
pub fn cmd_build(build_type: BuildType) -> Result<()> {
    let config = build_config()?;
    let cache_dir = config.project.cache_dir.as_str();

    Conan::from_config(&config)?.install(cache_dir, cache_dir)?;
    CMake::build(build_type, cache_dir, cache_dir)?;

    println!("Build successful!");
    Ok(())
}

/// Run the current sandbox project.
/// If the binary does not exist or is out of date, rebuild first, then run.
pub fn cmd_run(build_type: BuildType) -> Result<()> {
    cmd_build(build_type)?;
    let config = build_config()?;
    let cache_dir = PathBuf::from(config.project.cache_dir);
    let binary_name = if cfg!(target_os = "windows") {
        format!("{}.exe", config.project.name)
    } else {
        config.project.name
    };

    let binary_path = cache_dir.join(binary_name);
    std::process::Command::new(binary_path).status()?;

    Ok(())
}

/// Remove artifacts that CPPPG has generated in the past
/// Checks if cpppg.toml exists to prevent accidental use outside of sandbox project
pub fn cmd_clean() -> Result<()> {
    let config = build_config()?;

    let cache_dir = PathBuf::from(config.project.cache_dir);

    if cache_dir.is_dir() {
        fs::remove_dir_all(&cache_dir)?;
    }

    println!("Removed all build artifacts in {}", cache_dir.display());

    Ok(())
}

fn build_config() -> Result<Config> {
    let config_file = PathBuf::from("cpppg.toml");
    let config = Config::from_file(&config_file).context("Failed to load cpppg.toml")?;
    let cache_dir = PathBuf::from(&config.project.cache_dir);
    if !cache_dir.is_dir() {
        fs::create_dir_all(&cache_dir)?;
    }
    let conanfile_path = cache_dir.join("conanfile.py");
    let cmake_path = cache_dir.join("CMakeLists.txt");

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
