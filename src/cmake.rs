use std::{fmt::Display, process::Command, str::FromStr};

use anyhow::{bail, Result};

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
}

impl Display for CMake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            r#"cmake_minimum_required(VERSION 3.15)
project({} LANGUAGES CXX)

set(CMAKE_CXX_STANDARD {})
set(CMAKE_CXX_STANDARD_REQUIRED {})

add_executable(sandbox main.cpp)
"#,
            self.project_name,
            self.cpp_standard,
            if self.export_compile_commands {
                "ON"
            } else {
                "OFF"
            }
        )
    }
}
