use std::{fmt::Display, path::Path};

use anyhow::{anyhow, Context, Result};
use log::{info, LevelFilter};

use crate::{
    cmake::CMake,
    command_runner::{binary_stream_mode, tool_stream_mode, CommandRunner},
    commands::{cmd_add, find_and_add_dependency},
    conan::Conan,
    config::{Config, TestingConfig},
    traits::ToFile,
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
            r#"#include <gtest/gtest.h>

TEST({}, hello_test)
{{
    EXPECT_EQ(2 + 2, 4);
}}"#,
            self.name
        )
    }
}

pub fn testing_init(
    runner: impl CommandRunner,
    lvl: LevelFilter,
    config: &mut Config,
) -> Result<()> {
    info!("Initialize tests");
    config.testing.enabled = true;
    runner
        .command("mkdir")
        .args(["-p", config.testing.dir.as_str()])
        .stream_mode(tool_stream_mode(lvl))
        .run()?;
    let gtest_main = Path::new("build/_c3pg_gtest_main.cpp");
    std::fs::write(
        &gtest_main,
        r#"#include <gtest/gtest.h>

int main(int argc, char** argv) {
    ::testing::InitGoogleTest(&argc, argv);
    return RUN_ALL_TESTS();
}"#,
    )
    .context("Could not write gtest main file")?;
    find_and_add_dependency(runner, "gtest", config)?;
    Ok(())
}

pub fn testing_add(
    runner: impl CommandRunner,
    lvl: LevelFilter,
    config: &TestingConfig,
    name: &str,
) -> Result<()> {
    info!("Adding new test file {name}");
    let path_str = format!("{}/test_{}.cpp", config.dir, name);
    let path = Path::new(&path_str);
    if path
        .try_exists()
        .context(format!("test file {name} is inaccessible"))?
    {
        let path_str = path
            .to_str()
            .ok_or(anyhow!("test file path is not valid unicode"))?;
        info!("{} already exists.", path_str);
        return Ok(());
    }
    runner
        .command("mkdir")
        .args(["-p", config.dir.as_str()])
        .stream_mode(tool_stream_mode(lvl))
        .run()?;
    TestSuite::new(name).to_file(path)?;

    Ok(())
}

pub fn testing_build(
    runner: impl CommandRunner,
    lvl: LevelFilter,
    config: &Config,
    filter: Option<&str>,
    jobs: Option<u8>,
) -> Result<()> {
    info!(
        "Building tests matching expression {} ({} jobs)",
        filter.unwrap_or_default(),
        jobs.unwrap_or(1)
    );
    let test_target = format!("{}_tests", config.project.name);
    runner
        .command("cmake")
        .args(["--build", "build", "--target", test_target.as_str(), "-j"])
        .stream_mode(tool_stream_mode(lvl))
        .run()?;
    Ok(())
}

pub fn testing_run(
    runner: impl CommandRunner,
    lvl: LevelFilter,
    config: &Config,
    filter: Option<&str>,
    jobs: Option<u8>,
) -> Result<()> {
    testing_build(&runner, lvl, config, filter, jobs)?;
    info!(
        "Running tests matching expression {} ({} jobs)",
        filter.unwrap_or_default(),
        jobs.unwrap_or(1)
    );
    runner
        .command("ctest")
        .args(["ctest", "--test-dir", "build", "--output-on-failure", "-j"])
        .stream_mode(binary_stream_mode(lvl))
        .run()?;
    Ok(())
}
