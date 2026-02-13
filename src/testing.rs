use std::{fmt::Display, fs, path::Path};

use anyhow::{Context, Result};
use log::LevelFilter;

use crate::{
    command_runner::{binary_stream_mode, tool_stream_mode, CommandRunner},
    config::{Config, TestingConfig},
    traits::ToFile,
    ui,
};

#[derive(Debug, ToFile)]
struct TestSuite {
    name: String,
}

impl TestSuite {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Display for TestSuite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            r"#include <gtest/gtest.h>

TEST({}, hello_test)
{{
    EXPECT_EQ(2 + 2, 4);
}}",
            self.name
        )
    }
}

/// Create a new gtest source file `test_<name>.cpp` in the test directory.
///
/// If the file already exists, this is a no-op.
///
/// # Errors
///
/// Returns an error if the test directory cannot be created or the file
/// cannot be written.
pub fn testing_add(config: &TestingConfig, name: &str) -> Result<()> {
    ui::status("Adding", &format!("test file {name}"));
    let dir = Path::new(&config.dir);
    let path = dir.join(format!("test_{name}.cpp"));
    if path
        .try_exists()
        .context(format!("test file {name} is inaccessible"))?
    {
        ui::warn(&format!("{} already exists, skipping", path.display()));
        return Ok(());
    }
    fs::create_dir_all(dir).context("Could not create test directory")?;
    TestSuite::new(name).to_file(&path)?;

    Ok(())
}

/// Build the project's test targets via `cmake --build --target <name>_tests`.
///
/// # Errors
///
/// Returns an error if the `cmake --build` command fails.
pub fn testing_build(
    runner: impl CommandRunner,
    lvl: LevelFilter,
    config: &Config,
    jobs: Option<u8>,
) -> Result<()> {
    ui::status("Building", &format!("tests ({} jobs)", jobs.unwrap_or(1)));
    let test_target = format!("{}_tests", config.project.name);
    let cache_dir = config.project.cache_dir.as_str();
    let mut args = vec![
        "--build".to_string(),
        cache_dir.to_string(),
        "--target".to_string(),
        test_target,
    ];
    if let Some(j) = jobs {
        args.push(format!("-j{j}"));
    }
    runner
        .command("cmake")
        .args(args.iter().map(String::as_str))
        .stream_mode(tool_stream_mode(lvl))
        .run()?
        .expect_success("Failed to build tests")?;
    Ok(())
}

/// Build and run the project's test suite via `cmake` and `ctest`.
///
/// # Errors
///
/// Returns an error if the test build or `ctest` execution fails.
pub fn testing_run(
    runner: impl CommandRunner,
    lvl: LevelFilter,
    config: &Config,
    filter: Option<&str>,
    jobs: Option<u8>,
) -> Result<()> {
    testing_build(&runner, lvl, config, jobs)?;
    ui::status(
        "Testing",
        &format!(
            "{} ({} jobs)",
            filter.unwrap_or("*"),
            jobs.unwrap_or(1)
        ),
    );
    let cache_dir = config.project.cache_dir.as_str();
    let mut args = vec!["--test-dir", cache_dir, "--output-on-failure"];
    if let Some(f) = filter {
        args.extend(["-R", f]);
    }
    let jobs_str;
    if let Some(j) = jobs {
        jobs_str = j.to_string();
        args.extend(["-j", jobs_str.as_str()]);
    }
    runner
        .command("ctest")
        .args(args)
        .stream_mode(binary_stream_mode(lvl))
        .run()?;
    Ok(())
}
