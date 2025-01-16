use std::{fmt::Display, fs, path::Path, process::Command, str::FromStr};

use anyhow::{bail, Context, Result};
use regex::Regex;

use crate::dependency::Dependency;

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

#[derive(Debug, Default, Clone)]
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

#[derive(Debug)]
pub struct CMake {
    pub project_name: String,
    pub cpp_standard: CppStandard,
    pub export_compile_commands: bool,
    dependencies: Vec<String>,
}

impl CMake {
    pub fn new(
        project_name: String,
        cpp_standard: CppStandard,
        export_compile_commands: bool,
    ) -> Self {
        Self {
            project_name,
            cpp_standard,
            export_compile_commands,
            dependencies: vec![],
        }
    }

    pub fn build(build_type: BuildType) -> Result<()> {
        // Step 1: cmake configure
        let cmake_configure = Command::new("cmake")
            .args([
                "-B",
                "build",
                "-DCMAKE_TOOLCHAIN_FILE=build/conan_toolchain.cmake",
                format!("-DCMAKE_BUILD_TYPE={}", build_type).as_str(),
                "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON",
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

    pub fn add_dependency(&mut self, dep: &Dependency) {
        self.dependencies.push(dep.name.clone());
    }

    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path).with_context(|| {
            format!(
                "Could not read from {}",
                path.as_ref().to_path_buf().display()
            )
        })?;
        Self::from_str(content.as_str()).with_context(|| {
            format!(
                "Could not parse CMake from {}",
                path.as_ref().to_path_buf().display()
            )
        })
    }

    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        Ok(std::fs::write(path, self.to_string())?)
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
                    result.push_str(dep); // Add the dependency name
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

include(${{CMAKE_BINARY_DIR}}/conan_toolchain.cmake)
{}

add_executable(sandbox main.cpp)
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

impl FromStr for CMake {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut project_name = String::new();
        let mut cpp_standard = CppStandard::default();
        let mut export_compile_commands = false;
        let mut dependencies = Vec::new();

        // Regex patterns for different parts
        let project_re = Regex::new(r"^project\((.*?) LANGUAGES CXX\)").unwrap();
        let cpp_standard_re = Regex::new(r"^set\(CMAKE_CXX_STANDARD (\d+)\)").unwrap();
        let export_compile_commands_re =
            Regex::new(r"^set\(CMAKE_EXPORT_COMPILE_COMMANDS (ON|OFF)\)").unwrap();
        let find_package_re = Regex::new(r"^find_package\((.*?)\)").unwrap();
        let link_lib_re = Regex::new(r"^target_link_libraries\(sandbox (.*?)_LIBRARIES\)").unwrap();
        let include_dir_re =
            Regex::new(r"^target_include_directories\(sandbox PRIVATE (.*?)_INCLUDE_DIRS\)")
                .unwrap();

        // Iterate over the lines and apply regex matching
        for line in s.lines() {
            if let Some(caps) = project_re.captures(line) {
                project_name = caps[1].to_string();
            } else if let Some(caps) = cpp_standard_re.captures(line) {
                cpp_standard = CppStandard::from_str(&caps[1])?;
            } else if let Some(caps) = export_compile_commands_re.captures(line) {
                export_compile_commands = caps[1] == *"ON";
            } else if let Some(caps) = find_package_re.captures(line) {
                dependencies.push(caps[1].to_string());
            } else if let Some(caps) = link_lib_re.captures(line) {
                dependencies.push(caps[1].to_string());
            } else if let Some(caps) = include_dir_re.captures(line) {
                dependencies.push(caps[1].to_string());
            }
        }

        // Check if all necessary fields were populated
        if project_name.is_empty() {
            bail!("Missing project name");
        }

        Ok(CMake {
            project_name,
            cpp_standard,
            export_compile_commands,
            dependencies,
        })
    }
}
