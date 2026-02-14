use anyhow::{anyhow, bail, Context, Result};
use log::LevelFilter;
use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    cli::TestArgs,
    cmake_gen,
    traits::{FromFile, ToFile},
};
use crate::{
    cmake::{BuildType, CMake, CppStandard, Sanitizers},
    command_runner::CommandRunner,
};
use crate::{
    command_runner::binary_stream_mode,
    conan::{self, Conan},
};
use crate::{
    config::{Config, TargetType},
    testing::{testing_add, testing_run},
    ui,
};

const MAIN_CPP_TEMPLATE: &str = r#"#include <iostream>

int main() {
    std::cout << "Hello from C3PG!" << std::endl;
    return 0;
}
"#;

/// Shared scaffolding: create dirs, write config and build files, optionally
/// write a starter `main.cpp`, and optionally `git init`.
///
/// `git_dir` is the path passed to `git init` (e.g. the sandbox name for
/// `new`, or `"."` for `init`).
fn scaffold_project(
    runner: &impl CommandRunner,
    project_path: &Path,
    name: &str,
    git_dir: Option<&str>,
    standard: CppStandard,
    write_main: bool,
) -> Result<()> {
    let src_dir = project_path.join("src");
    fs::create_dir_all(&src_dir).context("Could not create src dir")?;
    let mut config = Config::default();
    let cache_dir = project_path.join(&config.project.cache_dir);
    fs::create_dir_all(&cache_dir).context("Could not create build dir")?;

    if write_main {
        fs::write(src_dir.join("main.cpp"), MAIN_CPP_TEMPLATE)
            .context("Could not write to main.cpp")?;
    }

    config.project.name = name.to_string();
    config.project.standard = standard;
    write_build_files(runner, &config, &cache_dir)?;
    config.to_file(project_path.join("c3pg.toml"))?;

    if let Some(dir) = git_dir {
        let gitignore_content = config.project.cache_dir.clone();
        fs::write(project_path.join(".gitignore"), gitignore_content)
            .context("Could not create gitignore")?;

        runner
            .command("git")
            .args(["init", "-b", "main", dir])
            .run()?
            .expect_success("Could not initialize git repository")?;
    }

    Ok(())
}

/// Create a new sandbox directory with a minimal setup (`CMakeLists.txt`,
/// `conanfile.py`, `main.cpp`).
///
/// # Errors
///
/// Returns an error if directory creation fails, config files cannot be
/// written, or `git init` fails when `git` is `true`.
pub fn cmd_new(
    runner: impl CommandRunner,
    sandbox_name: &str,
    git: bool,
    standard: CppStandard,
) -> Result<()> {
    let project_path = PathBuf::from(sandbox_name);
    let git_dir = if git { Some(sandbox_name) } else { None };
    scaffold_project(&runner, &project_path, sandbox_name, git_dir, standard, true)?;
    ui::status("Created", sandbox_name);
    Ok(())
}

/// Initialize a c3pg project in the current directory.
///
/// If `src/` is empty, writes a starter `main.cpp`. If sources already exist,
/// leaves them untouched.
///
/// # Errors
///
/// Returns an error if `c3pg.toml` already exists, directory creation fails,
/// or `git init` fails when `git` is `true`.
pub fn cmd_init(
    runner: impl CommandRunner,
    git: bool,
    standard: CppStandard,
) -> Result<()> {
    let project_path = std::env::current_dir().context("could not determine working directory")?;

    if project_path.join("c3pg.toml").exists() {
        bail!("c3pg.toml already exists -- project is already initialized");
    }

    let name = project_path
        .file_name()
        .map_or_else(|| "sandbox".to_string(), |n| n.to_string_lossy().into_owned());

    // Only write main.cpp if src/ doesn't already have source files
    let src_dir = project_path.join("src");
    let has_sources = src_dir.is_dir()
        && fs::read_dir(&src_dir)
            .map(|rd| rd.count() > 0)
            .unwrap_or(false);

    let git_dir = if git { Some(".") } else { None };
    scaffold_project(&runner, &project_path, &name, git_dir, standard, !has_sources)?;
    ui::status("Initialized", &name);
    Ok(())
}

/// Add a Conan dependency to the current project.
///
/// # Errors
///
/// Returns an error if the project config cannot be loaded, the dependency
/// cannot be found in the configured Conan remotes, or config files cannot
/// be written.
pub fn cmd_add(runner: impl CommandRunner, expr: &str) -> Result<()> {
    let mut config = build_config(&runner)?;
    find_and_add_dependency(&runner, expr, &mut config, Path::new("."))?;
    config.to_file("c3pg.toml")?;
    conan::remove_lockfile();
    Ok(())
}

/// Remove a Conan dependency from the current project.
///
/// # Errors
///
/// Returns an error if the project config cannot be loaded, config files
/// cannot be written, or the named dependency does not exist in the project.
pub fn cmd_remove(runner: impl CommandRunner, expr: &str) -> Result<()> {
    let mut config = build_config(&runner)?;
    let len_before = config.project.dependencies.len();
    config
        .project
        .dependencies
        .retain(|dep| dep.name.as_str() != expr);

    if config.project.dependencies.len() == len_before {
        bail!(
            "Dependency {} not found in project. Not removing anything...",
            expr
        );
    }

    let cache_dir = PathBuf::from(&config.project.cache_dir);
    write_build_files(&runner, &config, &cache_dir)?;
    config.to_file("c3pg.toml")?;
    conan::remove_lockfile();

    ui::status("Removed", &format!("dependency '{expr}'"));
    Ok(())
}

/// Build the current sandbox project.
///
/// Runs `conan install` followed by `cmake configure` and `cmake --build`.
///
/// # Errors
///
/// Returns an error if the project config cannot be loaded, Conan install
/// fails, or the cmake build fails.
pub fn cmd_build(
    runner: impl CommandRunner,
    build_type: BuildType,
    lvl: LevelFilter,
    sanitizers: &Sanitizers,
) -> Result<()> {
    sanitizers.validate()?;
    let config = build_config(&runner)?;
    cmd_build_inner(&runner, &config, build_type, lvl, sanitizers)
}

fn cmd_build_inner(
    runner: &impl CommandRunner,
    config: &Config,
    build_type: BuildType,
    lvl: LevelFilter,
    sanitizers: &Sanitizers,
) -> Result<()> {
    crate::diagnostics::preflight_build(runner);

    let cache_dir = PathBuf::from(&config.project.cache_dir);
    let cache_dir_str = cache_dir.display().to_string();

    let conan = Conan::from_config(runner, config)?;
    conan
        .install(runner, &cache_dir_str, &cache_dir_str, build_type, lvl)
        .context("conan install failed")?;

    // Capture resolved dependency versions into c3pg.lock so subsequent
    // builds are reproducible.
    let conanfile = cache_dir.join("conanfile.py");
    if let Err(e) = conan.lock_create(runner, &conanfile.display().to_string(), lvl) {
        // Non-fatal: a failed lock create should not block the build.
        ui::warn(&format!("could not create lockfile: {e}"));
    }

    // Read the build environment (CC, CXX, ...) that Conan generated so
    // cmake picks up the correct compiler.
    let build_env = conan::parse_conan_build_env(&cache_dir);
    CMake::build(
        runner,
        build_type,
        &cache_dir,
        &cache_dir,
        lvl,
        &build_env,
        sanitizers,
    )
    .context("cmake build failed")?;

    ui::status("Finished", "build");
    Ok(())
}

/// Run the current sandbox project.
///
/// If the binary does not exist or is out of date, rebuilds first, then runs.
/// When the project declares multiple executable targets, `target` selects
/// which one to run. With a single executable (or no explicit targets) the
/// `target` argument is optional.
///
/// # Errors
///
/// Returns an error if the build step fails, no executable target can be
/// determined, or the compiled binary cannot be executed.
pub fn cmd_run(
    runner: impl CommandRunner,
    build_type: BuildType,
    lvl: LevelFilter,
    sanitizers: &Sanitizers,
    target: Option<&str>,
) -> Result<()> {
    sanitizers.validate()?;
    let config = build_config(&runner)?;
    cmd_build_inner(&runner, &config, build_type, lvl, sanitizers)?;

    let exe_name = resolve_run_target(&config, target)?;

    let cache_dir = PathBuf::from(&config.project.cache_dir);
    let binary_name = if cfg!(target_os = "windows") {
        format!("{exe_name}.exe")
    } else {
        exe_name
    };

    let binary_path = cache_dir.join(binary_name);
    runner
        .command(binary_path.to_string_lossy())
        .stream_mode(binary_stream_mode(lvl))
        .run()?
        .expect_success_with_stdout(
            &format!("failed to run {}", binary_path.display()),
        )?;

    Ok(())
}

/// Determine which executable target to run.
///
/// In convention mode (no `[[targets]]`), the executable is the project name.
/// With explicit targets: if `target` is `Some`, it must name an executable;
/// if `None` and there is exactly one executable, use it; otherwise error.
fn resolve_run_target(config: &Config, target: Option<&str>) -> Result<String> {
    // Convention mode: no explicit targets, exe is project name.
    if config.targets.is_empty() {
        if let Some(name) = target {
            if name != config.project.name {
                bail!(
                    "target '{name}' not found (convention target is '{}')",
                    config.project.name
                );
            }
        }
        return Ok(config.project.name.clone());
    }

    let exe_targets: Vec<&str> = config
        .targets
        .iter()
        .filter(|t| t.target_type == TargetType::Executable)
        .map(|t| t.name.as_str())
        .collect();

    if let Some(name) = target {
        if exe_targets.contains(&name) {
            return Ok(name.to_string());
        }
        bail!(
            "target '{}' is not an executable target\navailable executables: {}",
            name,
            exe_targets.join(", ")
        );
    }

    match exe_targets.len() {
        0 => bail!("no executable targets found in c3pg.toml"),
        1 => Ok(exe_targets[0].to_string()),
        _ => bail!(
            "multiple executable targets found; use --target to choose one:\n  {}",
            exe_targets.join("\n  ")
        ),
    }
}

/// Run or manage the project's test suite.
///
/// With a subcommand (e.g. `add`), creates a new test file and lazily adds
/// gtest if it is not already a dependency. Without a subcommand, builds
/// and runs the test suite via `cmake` and `ctest`.
///
/// # Errors
///
/// Returns an error if the project config cannot be loaded, the test file
/// cannot be created, or the test build/run fails.
pub fn cmd_test(runner: impl CommandRunner, args: TestArgs, lvl: LevelFilter) -> Result<()> {
    args.sanitizers.validate()?;
    let mut config = build_config(&runner)?;
    if let Some(crate::cli::TestOnlySubcmds::Add { name }) = args.command {
        // Lazily add gtest on first test creation
        if !config
            .project
            .dependencies
            .iter()
            .any(|d| d.name == "gtest")
        {
            find_and_add_dependency(&runner, "gtest", &mut config, Path::new("."))?;
        }
        testing_add(&config.testing, &name)?;
        // Regenerate build files to pick up the new test file
        let cache_dir = PathBuf::from(&config.project.cache_dir);
        write_build_files(&runner, &config, &cache_dir)?;
        config.to_file("c3pg.toml")?;
        conan::remove_lockfile();
    } else {
        // Auto-detect: only run if test files exist
        let test_dir = Path::new(&config.testing.dir);
        if !test_dir.is_dir()
            || fs::read_dir(test_dir)
                .map(|rd| rd.count() == 0)
                .unwrap_or(true)
        {
            ui::status("Info", "no tests found -- use `c3pg test add <name>` to create one");
            return Ok(());
        }
        testing_run(runner, lvl, &config, args.filter.as_deref(), args.jobs)
            .context("test failed")?;
    }
    Ok(())
}

/// Remove artifacts that c3pg has generated in the past.
///
/// Checks if `c3pg.toml` exists to prevent accidental use outside of a
/// sandbox project.
///
/// # Errors
///
/// Returns an error if the project config cannot be loaded or the cache
/// directory cannot be removed.
pub fn cmd_clean(runner: impl CommandRunner) -> Result<()> {
    let config = build_config(&runner)?;

    let cache_dir = PathBuf::from(config.project.cache_dir);

    if cache_dir.is_dir() {
        fs::remove_dir_all(&cache_dir)?;
    }

    ui::status("Cleaned", &cache_dir.display().to_string());

    Ok(())
}

/// Search for a Conan dependency matching `expr`, add it to `config`, and
/// regenerate the `conanfile.py` and `CMakeLists.txt` under `project_root`.
///
/// # Errors
///
/// Returns an error if no matching dependency is found in the configured
/// remotes, or if the generated config files cannot be written.
pub fn find_and_add_dependency(
    runner: &impl CommandRunner,
    expr: &str,
    config: &mut Config,
    project_root: &Path,
) -> Result<()> {
    let dependency = Conan::from_config(runner, config)?
        .get_latest_matching_dependency(runner, expr)?
        .ok_or(anyhow!("Could not find dependency {} in remotes", expr))?;

    config.project.add_dependency(dependency.clone());

    let cache_dir = project_root.join(&config.project.cache_dir);
    write_build_files(runner, config, &cache_dir)?;

    ui::status("Added", &format!("dependency '{dependency}'"));
    Ok(())
}

/// Regenerate `conanfile.py` and `CMakeLists.txt` in `cache_dir` from `config`.
fn write_build_files(
    runner: &impl CommandRunner,
    config: &Config,
    cache_dir: &Path,
) -> Result<()> {
    Conan::from_config(runner, config)?.to_file(cache_dir.join("conanfile.py"))?;
    fs::write(
        cache_dir.join("CMakeLists.txt"),
        cmake_gen::generate_cmakelists(config)?,
    )
    .context("Could not write CMakeLists.txt")?;
    Ok(())
}

/// Load the project config from `c3pg.toml` (or legacy `cpppg.toml`),
/// create the cache directory, and regenerate build files.
///
/// # Errors
///
/// Returns an error if neither config file can be read, the cache directory
/// cannot be created, or the build files cannot be written.
pub fn load_config(runner: &impl CommandRunner) -> Result<Config> {
    build_config(runner)
}

fn build_config(runner: &impl CommandRunner) -> Result<Config> {
    let config_file = PathBuf::from("c3pg.toml");
    let legacy_file = PathBuf::from("cpppg.toml");
    let config = match Config::from_file(&config_file) {
        Ok(cfg) => cfg,
        Err(_) => {
            Config::from_file(&legacy_file).context("Failed to load c3pg.toml or cpppg.toml")?
        }
    };
    let cache_dir = PathBuf::from(&config.project.cache_dir);
    fs::create_dir_all(&cache_dir)?;
    write_build_files(runner, &config, &cache_dir)?;
    Ok(config)
}
