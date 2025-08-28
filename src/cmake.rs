use anyhow::{bail, Result};
use log::LevelFilter;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, str::FromStr};

use crate::{
    command_runner::{tool_stream_mode, CommandRunner},
    config::Config,
    traits::ToFile,
};

#[derive(Debug, Default, Clone)]
pub enum BuildType {
    #[default]
    Debug,
    RelWithDebInfo,
    Release,
}

impl Display for BuildType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FromStr for BuildType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "debug" => Ok(BuildType::Debug),
            "relwithdebinfo" => Ok(BuildType::RelWithDebInfo),
            "rel_with_deb_info" => Ok(BuildType::RelWithDebInfo),
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

#[derive(Debug, ToFile)]
pub struct CMake {
    pub project_name: String,
    pub cpp_standard: CppStandard,
    pub export_compile_commands: bool,
    pub enable_tests: bool,
}

impl CMake {
    pub fn build(
        command_runner: &impl CommandRunner,
        build_type: BuildType,
        build_dir: &str,
        src_dir: &str,
        lvl: LevelFilter,
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
                .map(|s| s.to_string()),
        );

        command_runner
            .command("cmake")
            .args(conf_args.iter().map(|s| s.as_str()))
            .stream_mode(tool_stream_mode(lvl))
            .run()?
            .expect_success("Failed to configure with cmake")?;

        // ---- Step 2: cmake --build ----
        let mut build_args = vec!["--build".into(), build_dir.into()];
        build_args.extend(
            cmake_build_verbosity_args(lvl)
                .iter()
                .map(|s| s.to_string()),
        );

        command_runner
            .command("cmake")
            .args(build_args.iter().map(|s| s.as_str()))
            .stream_mode(tool_stream_mode(lvl))
            .run()?
            .expect_success("Failed to build with cmake")?;

        Ok(())
    }

    pub fn from_config(config: &Config) -> Self {
        Self {
            project_name: config.project.name.clone(),
            cpp_standard: config.cmake.standard.clone(),
            export_compile_commands: config.cmake.export_compile_commands,
            enable_tests: config.testing.enabled,
        }
    }
}

impl Display for CMake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            r#"cmake_minimum_required(VERSION 3.21)

project({name} LANGUAGES CXX)

set(CMAKE_CXX_STANDARD {std})
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS {export_cmds})

include("${{CMAKE_BINARY_DIR}}/conandeps_legacy.cmake")

# Collect sources from ../src relative to this CMakeLists.txt
file(GLOB_RECURSE PROJECT_SOURCES CONFIGURE_DEPENDS
    "${{CMAKE_CURRENT_LIST_DIR}}/../src/*.c"
    "${{CMAKE_CURRENT_LIST_DIR}}/../src/*.cc"
    "${{CMAKE_CURRENT_LIST_DIR}}/../src/*.cxx"
    "${{CMAKE_CURRENT_LIST_DIR}}/../src/*.cpp"
    "${{CMAKE_CURRENT_LIST_DIR}}/../src/*.m"
    "${{CMAKE_CURRENT_LIST_DIR}}/../src/*.mm"
    "${{CMAKE_CURRENT_LIST_DIR}}/../src/*.h"
    "${{CMAKE_CURRENT_LIST_DIR}}/../src/*.hpp"
    "${{CMAKE_CURRENT_LIST_DIR}}/../src/*.hh"
    "${{CMAKE_CURRENT_LIST_DIR}}/../src/*.hxx"
)
list(FILTER PROJECT_SOURCES EXCLUDE REGEX ".*/main\\.cpp$")

add_library(lib{name} ${{PROJECT_SOURCES}})
add_executable({name} ${{CMAKE_CURRENT_LIST_DIR}}/../src/main.cpp)

target_link_libraries(lib{name} ${{CONANDEPS_LEGACY}})
target_link_libraries({name} lib{name})

"#,
            name = self.project_name,
            std = self.cpp_standard,
            export_cmds = if self.export_compile_commands {
                "ON"
            } else {
                "OFF"
            },
        )?;
        if self.enable_tests {
            write!(
                f,
                r#"# ---- tests ----
include(CTest)                     # defines BUILD_TESTING
if(BUILD_TESTING)
  include(GoogleTest)              # provides gtest_discover_tests()

  # Create a tiny main() so we don't depend on a gtest_main component name
  set(C3PG_GTEST_MAIN "${{CMAKE_CURRENT_BINARY_DIR}}/_c3pg_gtest_main.cpp")
  file(WRITE "${{C3PG_GTEST_MAIN}}" [=[
    #include <gtest/gtest.h>
    int main(int argc, char** argv) {{
      ::testing::InitGoogleTest(&argc, argv);
      return RUN_ALL_TESTS();
    }}
  ]=])

  # Non-recursive: only files directly in tests/ that match test_*.cpp
  file(GLOB TEST_FILES
       "${{CMAKE_CURRENT_LIST_DIR}}/../tests/test_*.cpp")

  # Aggregate target (optional but convenient)
  add_custom_target({name}_tests)

  foreach(test_src IN LISTS TEST_FILES)
    # Derive a safe target name from the file name (e.g., test_math.cpp -> test_math)
    get_filename_component(test_base "${{test_src}}" NAME_WE)
    string(MAKE_C_IDENTIFIER "${{test_base}}" test_target)

    add_executable("${{test_target}}" "${{test_src}}" "${{C3PG_GTEST_MAIN}}")
    target_link_libraries("${{test_target}}" PRIVATE lib{name} gtest::gtest)
    target_compile_features("${{test_target}}" PRIVATE cxx_std_{std})

    # Register individual tests with CTest; prefix with the file's base name
    gtest_discover_tests("${{test_target}}"
      TEST_PREFIX "${{test_base}}."
      DISCOVERY_MODE PRE_TEST
    )

    add_dependencies({name}_tests "${{test_target}}")
  endforeach()
endif()"#,
                name = self.project_name,
                std = self.cpp_standard,
            )?;
        }
        Ok(())
    }
}

fn cmake_configure_verbosity_args(lvl: LevelFilter) -> &'static [&'static str] {
    // CMake >=3.15 supports --log-level: ERROR|WARNING|NOTICE|STATUS|VERBOSE|DEBUG|TRACE
    match lvl {
        LevelFilter::Off | LevelFilter::Error => &["--log-level", "ERROR"],
        LevelFilter::Warn => &["--log-level", "ERROR"],
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
    use crate::{config::*, dependency::Dependency, test_utils::MockCommandRunner};

    #[test]
    fn test_cmake_build() {
        let mock_runner = MockCommandRunner::default();

        // Call the build function
        let result = CMake::build(
            &mock_runner,
            BuildType::Debug,
            "build_dir",
            "src_dir",
            LevelFilter::Info,
        );

        // Assert that the build completed successfully
        assert!(result.is_ok());

        // Check the recorded commands
        let commands = mock_runner.executed_commands();

        // Assert that two commands were executed
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

        // Call the build function with `Release` build type
        let result = CMake::build(
            &mock_runner,
            BuildType::Release,
            "release_build_dir",
            "src_dir",
            LevelFilter::Info,
        );

        // Assert that the build completed successfully
        assert!(result.is_ok());

        // Check the recorded commands
        let commands = mock_runner.executed_commands();

        // Assert that the correct build type was passed to the configure command
        assert_eq!(commands[0].0, "cmake");
        assert!(commands[0]
            .1
            .contains(&"-DCMAKE_BUILD_TYPE=Release".to_string()));

        // Validate the build command
        assert_eq!(commands[1].0, "cmake");
        assert_eq!(commands[1].1, vec!["--build", "release_build_dir"]);
    }

    #[test]
    fn test_cmake_fmt_no_dependencies() {
        let cmake = CMake {
            project_name: "NoDepsProject".to_string(),
            cpp_standard: CppStandard::Cpp17,
            export_compile_commands: false,
            enable_tests: false,
        };

        let cmake_string = cmake.to_string();

        // Validate basic project configuration
        assert!(cmake_string.contains("project(NoDepsProject LANGUAGES CXX)"));
        assert!(cmake_string.contains("set(CMAKE_CXX_STANDARD 17)"));
        assert!(cmake_string.contains("set(CMAKE_EXPORT_COMPILE_COMMANDS OFF)"));

        // Ensure no dependency-specific commands are present
        assert!(!cmake_string.contains("find_package"));
        assert!(!cmake_string.contains("target_include_directories"));
    }

    #[test]
    fn test_cmake_from_config_custom_values() {
        let config = Config {
            project: Project {
                name: "CustomProject".to_string(),
                dependencies: vec![
                    Dependency {
                        name: "Dependency1".to_string(),
                        ..Default::default()
                    },
                    Dependency {
                        name: "Dependency2".to_string(),
                        ..Default::default()
                    },
                ],
                cache_dir: "custom_cache".to_string(),
            },
            cmake: CMakeConfig {
                standard: CppStandard::Cpp17,
                export_compile_commands: false,
                silent: false,
            },
            conan: ConanConfig {
                bin: "custom_conan".to_string(),
                remote: Some("custom_remote".to_string()),
                silent: false,
            },
            testing: TestingConfig::default(),
        };

        let cmake = CMake::from_config(&config);

        // Validate project name
        assert_eq!(cmake.project_name, "CustomProject");

        // Validate C++ standard
        assert_eq!(cmake.cpp_standard, CppStandard::Cpp17);

        // Validate compile_commands setting
        assert!(!cmake.export_compile_commands);
    }
}
