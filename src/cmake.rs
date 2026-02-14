use anyhow::{bail, Context, Result};
use clap::Args;
use log::LevelFilter;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, path::Path, str::FromStr};

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

impl CppStandard {
    /// Returns true when the value is the default (`Cpp20`).  Used by serde
    /// `skip_serializing_if` to omit the field when unset.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
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

/// Sanitizer flags that translate to compiler/linker `-fsanitize=` options.
#[derive(Clone, Debug, Default, Args)]
pub struct Sanitizers {
    /// Enable `AddressSanitizer` (memory error detector)
    #[arg(long)]
    pub asan: bool,
    /// Enable `ThreadSanitizer` (data race detector)
    #[arg(long)]
    pub tsan: bool,
    /// Enable `UndefinedBehaviorSanitizer`
    #[arg(long)]
    pub ubsan: bool,
}

impl Sanitizers {
    /// Build the combined `-fsanitize=...` flag string.
    /// Returns `None` when no sanitizers are enabled.
    #[must_use]
    pub fn flag_string(&self) -> Option<String> {
        let mut parts = Vec::new();
        if self.asan {
            parts.push("address");
        }
        if self.tsan {
            parts.push("thread");
        }
        if self.ubsan {
            parts.push("undefined");
        }
        if parts.is_empty() {
            return None;
        }
        Some(format!("-fsanitize={}", parts.join(",")))
    }

    /// Return the extra cmake `-D` arguments needed to enable the selected
    /// sanitizers.  Returns an empty vec when nothing is enabled.
    #[must_use]
    pub fn cmake_args(&self) -> Vec<String> {
        let Some(flag) = self.flag_string() else {
            return Vec::new();
        };
        let mut cxx_flags = flag.clone();
        if self.asan {
            cxx_flags.push_str(" -fno-omit-frame-pointer");
        }
        vec![
            format!("-DCMAKE_CXX_FLAGS={cxx_flags}"),
            format!("-DCMAKE_EXE_LINKER_FLAGS={flag}"),
        ]
    }

    /// Returns an error if conflicting sanitizers are enabled.
    ///
    /// # Errors
    ///
    /// Returns an error when asan and tsan are both enabled, since they
    /// cannot be combined in GCC / Clang.
    pub fn validate(&self) -> Result<()> {
        if self.asan && self.tsan {
            bail!("AddressSanitizer and ThreadSanitizer cannot be combined");
        }
        Ok(())
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
        build_dir: &Path,
        src_dir: &Path,
        lvl: LevelFilter,
        build_env: &[(String, String)],
        sanitizers: &Sanitizers,
    ) -> Result<()> {
        let build_dir_str = build_dir.display().to_string();
        let src_dir_str = src_dir.display().to_string();

        // Build an absolute path so cmake doesn't resolve it relative to
        // the build directory (which would double-up the prefix).
        let toolchain = std::env::current_dir()
            .context("could not determine working directory")?
            .join(build_dir)
            .join("conan_toolchain.cmake");

        // ---- Step 1: cmake configure ----
        let mut conf_args = vec![
            "-B".into(),
            build_dir_str.clone(),
            format!("-DCMAKE_TOOLCHAIN_FILE={}", toolchain.display()),
            format!("-DCMAKE_BUILD_TYPE={build_type}"),
            "-S".into(),
            src_dir_str,
        ];
        conf_args.extend(sanitizers.cmake_args());
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
        let mut build_args = vec!["--build".into(), build_dir_str];
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
    use std::path::Path;

    #[test]
    fn test_cmake_build() {
        let mock_runner = MockCommandRunner::default();

        let result = CMake::build(
            &mock_runner,
            BuildType::Debug,
            Path::new("build_dir"),
            Path::new("src_dir"),
            LevelFilter::Info,
            &[],
            &Sanitizers::default(),
        );

        assert!(result.is_ok());

        let commands = mock_runner.executed_commands();
        assert_eq!(commands.len(), 2);

        // Validate the cmake configure command
        let conf = &commands[0];
        assert_eq!(conf.0, "cmake");
        assert!(conf.1.contains(&"-B".to_string()));
        assert!(conf.1.contains(&"build_dir".to_string()));
        assert!(conf.1.contains(&"-DCMAKE_BUILD_TYPE=Debug".to_string()));
        assert!(conf.1.contains(&"-S".to_string()));
        assert!(conf.1.contains(&"src_dir".to_string()));
        // Toolchain path is absolute; just verify the flag is present and ends correctly
        let tc_arg = conf
            .1
            .iter()
            .find(|a| a.starts_with("-DCMAKE_TOOLCHAIN_FILE="))
            .unwrap();
        assert!(
            tc_arg.ends_with("build_dir/conan_toolchain.cmake"),
            "Expected toolchain arg to end with build_dir/conan_toolchain.cmake, got: {tc_arg}"
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
            Path::new("release_build_dir"),
            Path::new("src_dir"),
            LevelFilter::Info,
            &[],
            &Sanitizers::default(),
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

    #[test]
    fn test_cmake_build_with_asan() {
        let mock_runner = MockCommandRunner::default();
        let san = Sanitizers {
            asan: true,
            tsan: false,
            ubsan: false,
        };

        let result = CMake::build(
            &mock_runner,
            BuildType::Debug,
            Path::new("build_dir"),
            Path::new("src_dir"),
            LevelFilter::Info,
            &[],
            &san,
        );

        assert!(result.is_ok());

        let commands = mock_runner.executed_commands();
        let conf_args = &commands[0].1;
        let cxx_flag = conf_args
            .iter()
            .find(|a| a.starts_with("-DCMAKE_CXX_FLAGS="))
            .expect("expected CMAKE_CXX_FLAGS");
        assert!(cxx_flag.contains("-fsanitize=address"));
        assert!(cxx_flag.contains("-fno-omit-frame-pointer"));

        let linker_flag = conf_args
            .iter()
            .find(|a| a.starts_with("-DCMAKE_EXE_LINKER_FLAGS="))
            .expect("expected CMAKE_EXE_LINKER_FLAGS");
        assert!(linker_flag.contains("-fsanitize=address"));
    }

    #[test]
    fn test_sanitizer_asan_tsan_conflict() {
        let san = Sanitizers {
            asan: true,
            tsan: true,
            ubsan: false,
        };
        assert!(san.validate().is_err());
    }

    #[test]
    fn test_sanitizer_no_flags_when_disabled() {
        let san = Sanitizers::default();
        assert!(san.cmake_args().is_empty());
        assert!(san.flag_string().is_none());
    }
}
