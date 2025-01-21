use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{fmt::Display, process::Command, str::FromStr};

use crate::{config::Config, dependency::Dependency, traits::ToFile};

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

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
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
    pub fn build(build_type: BuildType, build_dir: &str, src_dir: &str) -> Result<()> {
        // Step 1: cmake configure
        let cmake_configure = Command::new("cmake")
            .args([
                "-B",
                build_dir,
                "-DCMAKE_TOOLCHAIN_FILE=conan_toolchain.cmake",
                format!("-DCMAKE_BUILD_TYPE={}", build_type).as_str(),
                "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON",
                "-S",
                src_dir,
            ])
            .status()?;
        if !cmake_configure.success() {
            bail!("CMake configure failed");
        }

        // Step 2: cmake --build
        let cmake_build = Command::new("cmake").args(["--build", "build"]).status()?;
        if !cmake_build.success() {
            bail!("CMake build failed");
        }
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
        let link_libs = format_dependencies("target_link_libraries(sandbox ${", "_LIBRARIES})");
        let include_dirs = format_dependencies(
            "target_include_directories(sandbox PRIVATE ${",
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

add_executable(sandbox ${{CMAKE_CURRENT_LIST_DIR}}/../src/main.cpp)
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
            link_libs,
            include_dirs
        )
    }
}
