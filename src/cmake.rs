use anyhow::{bail, Result};
use log::LevelFilter;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, str::FromStr};

use crate::command_runner::{tool_stream_mode, CommandRunner};

#[derive(Debug, Default, Clone, Copy)]
pub enum BuildType {
    #[default]
    Debug,
    RelWithDebInfo,
    Release,
}

impl Display for BuildType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl FromStr for BuildType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "debug" => Ok(BuildType::Debug),
            "relwithdebinfo" | "rel_with_deb_info" => Ok(BuildType::RelWithDebInfo),
            "release" => Ok(BuildType::Release),
            _ => bail!("Could not match string {} to BuildType", s),
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub enum CppStandard {
    Cpp03,
    Cpp11,
    Cpp14,
    Cpp17,
    #[default]
    Cpp20,
    Cpp23,
}

impl Display for CppStandard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CppStandard::Cpp03 => write!(f, "03"),
            CppStandard::Cpp11 => write!(f, "11"),
            CppStandard::Cpp14 => write!(f, "14"),
            CppStandard::Cpp17 => write!(f, "17"),
            CppStandard::Cpp20 => write!(f, "20"),
            CppStandard::Cpp23 => write!(f, "23"),
        }
    }
}

impl FromStr for CppStandard {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "03" => Ok(CppStandard::Cpp03),
            "11" => Ok(CppStandard::Cpp11),
            "14" => Ok(CppStandard::Cpp14),
            "17" => Ok(CppStandard::Cpp17),
            "20" => Ok(CppStandard::Cpp20),
            "23" => Ok(CppStandard::Cpp23),
            _ => bail!("Could not match string {} to CppStandard", s),
        }
    }
}

pub struct CMake;

impl CMake {
    /// Run `cmake --configure` followed by `cmake --build`.
    ///
    /// # Errors
    ///
    /// Returns an error if either the configure or build step fails (e.g.
    /// `cmake` is not installed, the toolchain file is missing, or the
    /// build itself has compilation errors).
    pub fn build(
        command_runner: &impl CommandRunner,
        build_type: BuildType,
        build_dir: &str,
        src_dir: &str,
        lvl: LevelFilter,
        build_env: &[(String, String)],
    ) -> Result<()> {
        // ---- Step 1: cmake configure ----
        let mut conf_args = vec![
            "-B".into(),
            build_dir.into(),
            "-DCMAKE_TOOLCHAIN_FILE=conan_toolchain.cmake".into(),
            format!("-DCMAKE_BUILD_TYPE={}", build_type),
            "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON".into(),
            "-S".into(),
            src_dir.into(),
        ];
        conf_args.extend(
            cmake_configure_verbosity_args(lvl)
                .iter()
                .map(std::string::ToString::to_string),
        );

        command_runner
            .command("cmake")
            .args(conf_args.iter().map(std::string::String::as_str))
            .envs(build_env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stream_mode(tool_stream_mode(lvl))
            .run()?
            .expect_success("Failed to configure with cmake")?;

        // ---- Step 2: cmake --build ----
        let mut build_args = vec!["--build".into(), build_dir.into()];
        build_args.extend(
            cmake_build_verbosity_args(lvl)
                .iter()
                .map(std::string::ToString::to_string),
        );

        command_runner
            .command("cmake")
            .args(build_args.iter().map(std::string::String::as_str))
            .envs(build_env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stream_mode(tool_stream_mode(lvl))
            .run()?
            .expect_success("Failed to build with cmake")?;

        Ok(())
    }
}

fn cmake_configure_verbosity_args(lvl: LevelFilter) -> &'static [&'static str] {
    // CMake >=3.15 supports --log-level: ERROR|WARNING|NOTICE|STATUS|VERBOSE|DEBUG|TRACE
    match lvl {
        LevelFilter::Off | LevelFilter::Error | LevelFilter::Warn => &["--log-level", "ERROR"],
        LevelFilter::Info => &["--log-level", "WARNING"],
        LevelFilter::Debug => &["--log-level", "STATUS"],
        LevelFilter::Trace => &["--log-level", "VERBOSE"],
    }
}

fn cmake_build_verbosity_args(lvl: LevelFilter) -> &'static [&'static str] {
    // Forward -v to the underlying build tool at higher levels
    match lvl {
        LevelFilter::Debug | LevelFilter::Trace => &["--", "-v"], // Ninja/Make verbose
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::MockCommandRunner;

    #[test]
    fn test_cmake_build() {
        let mock_runner = MockCommandRunner::default();

        let result = CMake::build(
            &mock_runner,
            BuildType::Debug,
            "build_dir",
            "src_dir",
            LevelFilter::Info,
            &[],
        );

        assert!(result.is_ok());

        let commands = mock_runner.executed_commands();
        assert_eq!(commands.len(), 2);

        // Validate the cmake configure command
        assert_eq!(commands[0].0, "cmake");
        assert_eq!(
            commands[0].1,
            vec![
                "-B",
                "build_dir",
                "-DCMAKE_TOOLCHAIN_FILE=conan_toolchain.cmake",
                "-DCMAKE_BUILD_TYPE=Debug",
                "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON",
                "-S",
                "src_dir",
                "--log-level",
                "WARNING"
            ]
        );

        // Validate the cmake build command
        assert_eq!(commands[1].0, "cmake");
        assert_eq!(commands[1].1, vec!["--build", "build_dir"]);
    }

    #[test]
    fn test_cmake_build_with_release_type() {
        let mock_runner = MockCommandRunner::default();

        let result = CMake::build(
            &mock_runner,
            BuildType::Release,
            "release_build_dir",
            "src_dir",
            LevelFilter::Info,
            &[],
        );

        assert!(result.is_ok());

        let commands = mock_runner.executed_commands();

        assert_eq!(commands[0].0, "cmake");
        assert!(commands[0]
            .1
            .contains(&"-DCMAKE_BUILD_TYPE=Release".to_string()));

        assert_eq!(commands[1].0, "cmake");
        assert_eq!(commands[1].1, vec!["--build", "release_build_dir"]);
    }
}
