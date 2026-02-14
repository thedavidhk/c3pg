use std::path::Path;

use anyhow::{bail, Result};
use log::LevelFilter;

use crate::{
    cmake_gen::{find_files, HEADER_EXTENSIONS, SOURCE_EXTENSIONS},
    command_runner::{tool_stream_mode, CommandRunner},
    ui,
};

/// Collect all C/C++ source and header files under `src/`, `include/`, and
/// the test directory (if present).
fn collect_project_files(test_dir: &str) -> Vec<String> {
    let all_exts: Vec<&str> = SOURCE_EXTENSIONS
        .iter()
        .chain(HEADER_EXTENSIONS.iter())
        .copied()
        .collect();

    let mut files = find_files("src", &all_exts);
    files.extend(find_files("include", &all_exts));
    let test_files = find_files(test_dir, &all_exts);
    files.extend(test_files);
    files
}

/// Run `clang-format` on all project source and header files.
///
/// When `check` is true, runs in dry-run mode and returns an error if any
/// files would be reformatted (useful for CI).
///
/// # Errors
///
/// Returns an error if no source files are found, `clang-format` is not
/// on `PATH`, or (in check mode) files are not correctly formatted.
pub fn cmd_fmt(
    runner: &impl CommandRunner,
    lvl: LevelFilter,
    test_dir: &str,
    check: bool,
) -> Result<()> {
    let files = collect_project_files(test_dir);
    if files.is_empty() {
        bail!("no source files found to format");
    }

    let mut args: Vec<&str> = if check {
        vec!["--dry-run", "--Werror"]
    } else {
        vec!["-i"]
    };
    args.extend(files.iter().map(String::as_str));

    if check {
        ui::status("Checking", &format!("{} files", files.len()));
    } else {
        ui::status("Formatting", &format!("{} files", files.len()));
    }

    runner
        .command("clang-format")
        .args(args)
        .stream_mode(tool_stream_mode(lvl))
        .run()?
        .expect_success("clang-format failed")?;

    if !check {
        ui::status("Finished", "format");
    }

    if !Path::new(".clang-format").exists() && !Path::new("_clang-format").exists() {
        ui::warn("no .clang-format file found -- using clang-format defaults");
    }

    Ok(())
}

/// Run `clang-tidy` on all project source files (not headers).
///
/// Requires `compile_commands.json` to exist in the build directory.
/// When `fix` is true, applies suggested fixes in-place.
///
/// # Errors
///
/// Returns an error if `compile_commands.json` is missing, no source files
/// are found, or `clang-tidy` reports errors.
pub fn cmd_lint(
    runner: &impl CommandRunner,
    lvl: LevelFilter,
    test_dir: &str,
    build_dir: &str,
    fix: bool,
) -> Result<()> {
    let compile_db = Path::new(build_dir).join("compile_commands.json");
    if !compile_db.exists() {
        bail!("compile_commands.json not found in {build_dir}/ -- run `c3pg build` first");
    }

    // clang-tidy operates on source files (not headers)
    let files = find_files("src", &SOURCE_EXTENSIONS);
    let test_files = find_files(test_dir, &SOURCE_EXTENSIONS);
    let all_files: Vec<String> = files.into_iter().chain(test_files).collect();

    if all_files.is_empty() {
        bail!("no source files found to lint");
    }

    ui::status("Linting", &format!("{} files", all_files.len()));

    let p_flag = format!("-p={build_dir}");
    let mut args = vec![p_flag.as_str()];
    if fix {
        args.push("--fix");
    }
    args.extend(all_files.iter().map(String::as_str));

    runner
        .command("clang-tidy")
        .args(args)
        .stream_mode(tool_stream_mode(lvl))
        .run()?
        .expect_success("clang-tidy failed")?;

    ui::status("Finished", "lint");
    Ok(())
}
