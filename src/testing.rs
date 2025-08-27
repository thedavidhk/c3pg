use anyhow::Result;
use log::LevelFilter;

use crate::{
    command_runner::{tool_stream_mode, CommandRunner},
    config::TestingConfig,
};

pub fn testing_init(
    runner: impl CommandRunner,
    lvl: LevelFilter,
    config: &mut TestingConfig,
) -> Result<()> {
    println!("Initialize tests");
    config.enabled = true;
    runner
        .command("mkdir")
        .args(["-p", config.dir.as_str()])
        .stream_mode(tool_stream_mode(lvl))
        .run()?;
    Ok(())
}

pub fn testing_add(
    runner: impl CommandRunner,
    lvl: LevelFilter,
    config: &TestingConfig,
    name: &str,
) -> Result<()> {
    println!("Adding new test file {name}");
    Ok(())
}

pub fn testing_run(
    runner: impl CommandRunner,
    lvl: LevelFilter,
    config: &TestingConfig,
    filter: Option<&str>,
    jobs: Option<u8>,
) -> Result<()> {
    println!(
        "Running tests matching expression {} ({} jobs)",
        filter.unwrap_or_default(),
        jobs.unwrap_or(1)
    );
    Ok(())
}
