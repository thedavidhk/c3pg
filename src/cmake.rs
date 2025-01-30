use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{fmt::Display, str::FromStr};

use crate::{
    command_runner::CommandRunner, config::Config, dependency::Dependency, traits::ToFile,
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
    dependencies: Vec<Dependency>,
}

impl CMake {
    pub fn build(
        command_runner: impl CommandRunner,
        build_type: BuildType,
        build_dir: &str,
        src_dir: &str,
    ) -> Result<()> {
        // Step 1: cmake configure
        let cmake_configure = command_runner
            .command("cmake")
            .args([
                "-B",
                build_dir,
                "-DCMAKE_TOOLCHAIN_FILE=conan_toolchain.cmake",
                format!("-DCMAKE_BUILD_TYPE={}", build_type).as_str(),
                "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON",
                "-S",
                src_dir,
            ])
            .run()?;
        cmake_configure.expect_success("CMake configure failed")?;

        // Step 2: cmake --build
        let cmake_build = command_runner
            .command("cmake")
            .args(["--build", "build"])
            .run()?;
        cmake_build.expect_success("CMake build failed")?;
        Ok(())
    }

    pub fn from_config(config: &Config) -> Self {
        Self {
            project_name: config.project.name.clone(),
            cpp_standard: config.cmake.standard.clone(),
            export_compile_commands: config.cmake.export_compile_commands,
            dependencies: config.project.dependencies.clone(),
        }
    }
}

impl Display for CMake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let format_dependencies = |prefix: &str, suffix: &str| {
            self.dependencies
                .iter()
                .map(|dep| {
                    let mut result = String::new();
                    result.push_str(prefix); // Start with the prefix
                    result.push_str(&dep.name); // Add the dependency name
                    result.push_str(suffix); // Add the suffix
                    result
                })
                .collect::<Vec<String>>()
                .join("\n")
        };

        let find_packages = format_dependencies("find_package(", ")");
        let link_libs = format_dependencies(
            format!("target_link_libraries({} ${{", self.project_name).as_str(),
            "_LIBRARIES})",
        );
        let include_dirs = format_dependencies(
            format!(
                "target_include_directories({} PRIVATE ${{",
                self.project_name
            )
            .as_str(),
            "_INCLUDE_DIRS})",
        );

        write!(
            f,
            r#"cmake_minimum_required(VERSION 3.15)

project({} LANGUAGES CXX)

set(CMAKE_CXX_STANDARD {})
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS {})

include(${{CMAKE_TOOLCHAIN_FILE}})
{}

add_executable({} ${{CMAKE_CURRENT_LIST_DIR}}/../src/main.cpp)
{}
{}
"#,
            self.project_name,
            self.cpp_standard,
            if self.export_compile_commands {
                "ON"
            } else {
                "OFF"
            },
            find_packages,
            self.project_name,
            link_libs,
            include_dirs
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::*, test_utils::MockCommandRunner};

    #[test]
    fn test_cmake_build() {
        let mock_runner = MockCommandRunner::default();

        // Call the build function
        let result = CMake::build(&mock_runner, BuildType::Debug, "build_dir", "src_dir");

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
                "src_dir"
            ]
        );

        // Validate the cmake build command
        assert_eq!(commands[1].0, "cmake");
        assert_eq!(commands[1].1, vec!["--build", "build"]);
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
        assert_eq!(commands[1].1, vec!["--build", "build"]);
    }

    #[test]
    fn test_cmake_fmt_no_dependencies() {
        let cmake = CMake {
            project_name: "NoDepsProject".to_string(),
            cpp_standard: CppStandard::Cpp17,
            export_compile_commands: false,
            dependencies: vec![],
        };

        let cmake_string = cmake.to_string();

        // Validate basic project configuration
        assert!(cmake_string.contains("project(NoDepsProject LANGUAGES CXX)"));
        assert!(cmake_string.contains("set(CMAKE_CXX_STANDARD 17)"));
        assert!(cmake_string.contains("set(CMAKE_EXPORT_COMPILE_COMMANDS OFF)"));

        // Ensure no dependency-specific commands are present
        assert!(!cmake_string.contains("find_package"));
        assert!(!cmake_string.contains("target_link_libraries"));
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
            },
            conan: ConanConfig {
                bin: "custom_conan".to_string(),
                remote: Some("custom_remote".to_string()),
            },
        };

        let cmake = CMake::from_config(&config);

        // Validate project name
        assert_eq!(cmake.project_name, "CustomProject");

        // Validate dependencies
        assert_eq!(
            cmake.dependencies,
            vec![
                Dependency {
                    name: "Dependency1".to_string(),
                    ..Default::default()
                },
                Dependency {
                    name: "Dependency2".to_string(),
                    ..Default::default()
                },
            ]
        );

        // Validate C++ standard
        assert_eq!(cmake.cpp_standard, CppStandard::Cpp17);

        // Validate compile_commands setting
        assert!(!cmake.export_compile_commands);
    }
}
