use anyhow::{anyhow, bail, Context, Result};
use log::{info, LevelFilter};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

use crate::{
    cli::TestArgs,
    cmake_gen,
    traits::{FromFile, ToFile},
};
use crate::{
    cmake::{BuildType, CMake, CppStandard},
    command_runner::CommandRunner,
};
use crate::{command_runner::binary_stream_mode, conan::Conan};
use crate::{
    config::Config,
    testing::{testing_add, testing_run},
};

/// Create a new sandbox directory with a minimal setup (CMakeLists.txt, conanfile.py, main.cpp).
pub fn cmd_new(
    runner: impl CommandRunner,
    sandbox_name: &str,
    git: bool,
    standard: CppStandard,
) -> Result<()> {
    // 1. Create the sandbox directory
    let project_path = PathBuf::from(sandbox_name);
    let src_dir = project_path.join("src");
    fs::create_dir_all(&src_dir).context("Could not create src dir")?;
    let mut config = Config::default();
    let cache_dir = project_path.join(&config.project.cache_dir);
    fs::create_dir_all(&cache_dir).context("Could not create build dir")?;

    // 2. Write main.cpp
    let main_cpp_content = r#"#include <iostream>

int main() {
    std::cout << "Hello from C3PG!" << std::endl;
    return 0;
}
"#;
    fs::write(src_dir.join("main.cpp"), main_cpp_content).context("Could not write to main.cpp")?;

    // 3. Write minimal config files
    config.project.name = sandbox_name.to_string();
    config.cmake.standard = standard;
    fs::write(
        cache_dir.join("CMakeLists.txt"),
        cmake_gen::generate_cmakelists(&config)?,
    )
    .context("Could not write CMakeLists.txt")?;
    Conan::from_config(&runner, &config)?.to_file(cache_dir.join("conanfile.py"))?;
    find_and_add_dependency(runner, "gtest", &mut config)?;
    config.to_file(project_path.join("c3pg.toml"))?;

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

    info!("Created new sandbox: {}", sandbox_name);
    Ok(())
}

/// Add a Conan dependency to the current project
pub fn cmd_add(runner: impl CommandRunner, expr: &str) -> Result<()> {
    let mut config = build_config(&runner)?;
    find_and_add_dependency(runner, expr, &mut config)?;
    config.to_file("c3pg.toml")
}

/// Remove a Conan dependency from the current project
pub fn cmd_remove(runner: impl CommandRunner, expr: &str) -> Result<()> {
    let mut config = build_config(&runner)?;
    let len_before = config.project.dependencies.len();
    config
        .project
        .dependencies
        .retain(|dep| dep.name.as_str() != expr);
    let len_after = config.project.dependencies.len();

    let cache_dir = PathBuf::from(&config.project.cache_dir);
    let conanfile_path = cache_dir.join("conanfile.py");
    let cmake_path = cache_dir.join("CMakeLists.txt");
    Conan::from_config(&runner, &config)?.to_file(conanfile_path)?;
    fs::write(&cmake_path, cmake_gen::generate_cmakelists(&config)?)
        .context("Could not write CMakeLists.txt")?;
    config.to_file("c3pg.toml")?;

    if len_before == len_after {
        bail!(
            "Dependency {} not found in project. Not removing anything...",
            expr
        );
    }

    info!("Removed dependency {}", expr);

    Ok(())
}

/// Build the current sandbox project.
pub fn cmd_build(
    runner: impl CommandRunner,
    build_type: BuildType,
    lvl: LevelFilter,
) -> Result<()> {
    let config = build_config(&runner)?;
    let cache_dir = config.project.cache_dir.as_str();

    Conan::from_config(&runner, &config)?.install(
        &runner,
        cache_dir,
        cache_dir,
        build_type.clone(),
        lvl,
    )?;
    CMake::build(&runner, build_type, cache_dir, cache_dir, lvl)?;

    info!("Build successful!\n");
    Ok(())
}

/// Run the current sandbox project.
/// If the binary does not exist or is out of date, rebuild first, then run.
pub fn cmd_run(runner: impl CommandRunner, build_type: BuildType, lvl: LevelFilter) -> Result<()> {
    cmd_build(&runner, build_type, lvl)?;
    let config = build_config(&runner)?;
    let cache_dir = PathBuf::from(config.project.cache_dir);
    let binary_name = if cfg!(target_os = "windows") {
        format!("{}.exe", config.project.name)
    } else {
        config.project.name
    };

    let binary_path = cache_dir.join(binary_name);
    runner
        .command(binary_path.to_string_lossy())
        .stream_mode(binary_stream_mode(lvl))
        .run()?
        .expect_success_with_stdout(
            format!("Could not run binary {}", binary_path.to_string_lossy()).as_str(),
        )?;

    Ok(())
}

pub fn cmd_test(runner: impl CommandRunner, args: TestArgs, lvl: LevelFilter) -> Result<()> {
    let config = build_config(&runner)?;
    match args.command {
        Some(command) => match command {
            crate::cli::TestOnlySubcmds::Add { name } => {
                testing_add(runner, lvl, &config.testing, &name)?
            }
        },
        None => testing_run(runner, lvl, &config, args.filter.as_deref(), args.jobs)?,
    };
    config.to_file("c3pg.toml")?;
    Ok(())
}

/// Remove artifacts that c3pg has generated in the past
/// Checks if c3pg.toml exists to prevent accidental use outside of sandbox project
pub fn cmd_clean(runner: impl CommandRunner) -> Result<()> {
    let config = build_config(&runner)?;

    let cache_dir = PathBuf::from(config.project.cache_dir);

    if cache_dir.is_dir() {
        fs::remove_dir_all(&cache_dir)?;
    }

    info!("Removed all build artifacts in {}", cache_dir.display());

    Ok(())
}

pub fn find_and_add_dependency(
    runner: impl CommandRunner,
    expr: &str,
    config: &mut Config,
) -> Result<()> {
    // Find dependency
    let dependency = Conan::from_config(&runner, config)?
        .get_latest_matching_dependency(&runner, expr)?
        .ok_or(anyhow!("Could not find dependency {} in remotes", expr))?;

    // 1. Read and parse existing conanfile.py and CMakeLists.txt
    let cache_dir = PathBuf::from(&config.project.cache_dir);
    let conanfile_path = cache_dir.join("conanfile.py");
    let cmake_path = cache_dir.join("CMakeLists.txt");

    // 2. Add dependency
    config.project.add_dependency(dependency.clone());
    Conan::from_config(&runner, config)?
        .to_file(conanfile_path)
        .with_context(|| "Could not write conan config")?;
    fs::write(&cmake_path, cmake_gen::generate_cmakelists(config)?)
        .with_context(|| "Could not write CMakeLists.txt")?;

    info!("Added dependency '{}'", dependency);
    Ok(())
}

fn build_config(runner: impl CommandRunner) -> Result<Config> {
    let config_file = PathBuf::from("c3pg.toml");
    let legacy_file = PathBuf::from("cpppg.toml");
    let config = match Config::from_file(&config_file) {
        Ok(cfg) => cfg,
        Err(_) => {
            Config::from_file(&legacy_file).context("Failed to load c3pg.toml or cpppg.toml")?
        }
    };
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
            Conan::from_config(&runner, &config)?.to_file(&conanfile_path)?;
        }
    } else {
        Conan::from_config(&runner, &config)?.to_file(&conanfile_path)?;
    }

    if cmake_path.exists() {
        let cmake_modified = fs::metadata(&cmake_path)?.modified()?;
        if cmake_modified < config_modified {
            fs::write(&cmake_path, cmake_gen::generate_cmakelists(&config)?)?;
        }
    } else {
        fs::write(&cmake_path, cmake_gen::generate_cmakelists(&config)?)?;
    }

    Ok(config)
}
