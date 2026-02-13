use std::{fmt::Display, path::Path};

use anyhow::{anyhow, Context, Result};
use log::{info, LevelFilter};

use crate::{
    command_runner::{binary_stream_mode, tool_stream_mode, CommandRunner},
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
    if !config.testing.enabled {
        info!("Testing is not enabled. You can enable it in c3pg.toml");
        return Ok(());
    }
    info!(
        "Building tests matching expression {} ({} jobs)",
        filter.unwrap_or_default(),
        jobs.unwrap_or(1)
    );
    let test_target = format!("{}_tests", config.project.name);
    let cache_dir = config.project.cache_dir.as_str();
    runner
        .command("cmake")
        .args(["--build", cache_dir, "--target", test_target.as_str(), "-j"])
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
    if !config.testing.enabled {
        info!("Testing is not enabled. You can enable it in c3pg.toml");
        return Ok(());
    }
    testing_build(&runner, lvl, config, filter, jobs)?;
    info!(
        "Running tests matching expression {} ({} jobs)",
        filter.unwrap_or_default(),
        jobs.unwrap_or(1)
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
