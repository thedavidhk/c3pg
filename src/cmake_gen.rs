use std::path::Path;

use crate::{
    cmake_core::{
        DiscoverMode, LibType, Project, Scope::PRIVATE, Target, TestEntry, TestFramework,
        TestSuite, Value,
    },
    config::Config,
};
use anyhow::Result;
use walkdir::WalkDir;

/// Generate a complete CMakeLists.txt string from the project configuration.
pub fn generate_cmakelists(config: &Config) -> Result<String> {
    let project_name = &config.project.name;
    let lib_name = format!("lib{project_name}");
    let std_str = config.cmake.standard.to_string();

    // Discover source files, making them relative to the CMakeLists.txt location
    let all_sources = find_files("src", &SOURCE_EXTENSIONS);
    let lib_sources: Vec<Value> = all_sources
        .iter()
        .filter(|p| !p.ends_with("main.cpp"))
        .map(|p| cmake_list_dir_value(p))
        .collect();

    let has_lib = !lib_sources.is_empty();

    let mut project = Project::new(project_name)
        .languages(&["CXX"])
        .set_var("CMAKE_CXX_STANDARD", std_str.as_str())
        .set_on("CMAKE_CXX_STANDARD_REQUIRED")
        .set_var(
            "CMAKE_EXPORT_COMPILE_COMMANDS",
            if config.cmake.export_compile_commands {
                "ON"
            } else {
                "OFF"
            },
        )
        .include("${CMAKE_BINARY_DIR}/conandeps_legacy.cmake");

    if has_lib {
        let lib = Target::library(&lib_name, LibType::Static)
            .srcs(lib_sources)
            .link(PRIVATE, Value::Raw("${CONANDEPS_LEGACY}".into()));
        let app = Target::executable(project_name)
            .src(cmake_list_dir_value("src/main.cpp"))
            .link(PRIVATE, &lib_name);
        project = project.target(lib).target(app);
    } else {
        let app = Target::executable(project_name)
            .src(cmake_list_dir_value("src/main.cpp"))
            .link(PRIVATE, Value::Raw("${CONANDEPS_LEGACY}".into()));
        project = project.target(app);
    }

    if config.testing.enabled {
        let gtest = TestFramework::GoogleTest {
            config_mode: true,
            inline_main_var: "C3PG_GTEST_MAIN".into(),
            inline_main_body: GTEST_MAIN_BODY.trim().into(),
            discover_mode: DiscoverMode::PreTest,
        };

        let test_files = find_files(&config.testing.dir, &SOURCE_EXTENSIONS);
        let cxx_std: u16 = std_str.parse().unwrap_or(20);

        let entries = test_files.iter().map(|path| {
            let base = Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("test");
            let safe_name = sanitize_to_c_identifier(base);
            let mut link = vec![Value::Raw("gtest::gtest".into())];
            if has_lib {
                link.insert(0, Value::Str(lib_name.clone()));
            } else {
                link.insert(0, Value::Raw("${CONANDEPS_LEGACY}".into()));
            }
            TestEntry {
                exe_name: safe_name.clone(),
                sources: vec![
                    cmake_list_dir_value(path),
                    Value::Raw("${C3PG_GTEST_MAIN}".into()),
                ],
                link,
                prefix: format!("{base}."),
                cxx_standard: Some(cxx_std),
            }
        });

        let tests_target = format!("{project_name}_tests");
        let suite = entries.fold(
            TestSuite::new_aggregate(&tests_target, gtest),
            |acc, e| acc.add(e),
        );

        project = project.with_tests(suite);
    }

    project.emit()
}

const GTEST_MAIN_BODY: &str = r#"
#include <gtest/gtest.h>
int main(int argc, char** argv) {
  ::testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}
"#;

const SOURCE_EXTENSIONS: [&str; 4] = ["c", "cpp", "cxx", "cc"];

/// Wrap a project-root-relative path as `${CMAKE_CURRENT_LIST_DIR}/../<path>`.
///
/// The generated CMakeLists.txt lives inside the cache/build directory, so all
/// references to project files need this prefix to resolve correctly.
fn cmake_list_dir_value(path: &str) -> Value {
    Value::Raw(format!("${{CMAKE_CURRENT_LIST_DIR}}/../{path}"))
}

/// Walk a directory tree and return paths (relative to CWD) of files whose
/// extension matches one of `exts`. Returns an empty vec if the directory
/// does not exist or is unreadable.
fn find_files(root: &str, exts: &[&str]) -> Vec<String> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| exts.contains(&ext))
                .unwrap_or(false)
        })
        .map(|e| e.into_path().to_string_lossy().into_owned())
        .collect()
}

fn sanitize_to_c_identifier(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        out.push(if ch.is_alphanumeric() || ch == '_' {
            ch
        } else {
            '_'
        });
    }
    if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        out.insert(0, '_');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cmake::CppStandard,
        config::{CMakeConfig, ConanConfig, Config, TestingConfig},
    };

    fn test_config(name: &str, standard: CppStandard, tests_enabled: bool) -> Config {
        Config {
            project: crate::config::Project {
                name: name.to_string(),
                dependencies: vec![],
                cache_dir: "build".to_string(),
            },
            cmake: CMakeConfig {
                standard,
                export_compile_commands: true,
                silent: false,
            },
            conan: ConanConfig::default(),
            testing: TestingConfig {
                enabled: tests_enabled,
                link: false,
                dir: "tests".to_string(),
            },
        }
    }

    #[test]
    fn test_generate_basic_project() {
        let config = test_config("MyProject", CppStandard::Cpp20, false);
        let output = generate_cmakelists(&config).unwrap();

        assert!(output.contains("cmake_minimum_required(VERSION 3.21)"));
        assert!(output.contains("project(MyProject LANGUAGES CXX)"));
        assert!(output.contains("set(CMAKE_CXX_STANDARD 20)"));
        assert!(output.contains("set(CMAKE_CXX_STANDARD_REQUIRED ON)"));
        assert!(output.contains("set(CMAKE_EXPORT_COMPILE_COMMANDS ON)"));
        assert!(output.contains("include(${CMAKE_BINARY_DIR}/conandeps_legacy.cmake)"));
        assert!(output.contains("add_executable(MyProject"));
        assert!(output.contains("${CMAKE_CURRENT_LIST_DIR}/../src/main.cpp"));

        // Without library sources, executable links directly to Conan deps
        // (no add_library since there are no lib-eligible source files)
        assert!(!output.contains("add_library("));
        assert!(output.contains("${CONANDEPS_LEGACY}"));

        // No test section when tests are disabled
        assert!(!output.contains("include(CTest)"));
        assert!(!output.contains("enable_testing()"));
    }

    #[test]
    fn test_generate_with_cpp17_and_no_export() {
        let mut config = test_config("NoDepsProject", CppStandard::Cpp17, false);
        config.cmake.export_compile_commands = false;
        let output = generate_cmakelists(&config).unwrap();

        assert!(output.contains("set(CMAKE_CXX_STANDARD 17)"));
        assert!(output.contains("set(CMAKE_EXPORT_COMPILE_COMMANDS OFF)"));
        assert!(output.contains("project(NoDepsProject LANGUAGES CXX)"));
    }

    #[test]
    fn test_generate_with_tests_enabled() {
        let config = test_config("TestProject", CppStandard::Cpp20, true);
        let output = generate_cmakelists(&config).unwrap();

        assert!(output.contains("include(CTest)"));
        assert!(output.contains("enable_testing()"));
        assert!(output.contains("find_package(GTest CONFIG REQUIRED)"));
        assert!(output.contains("include(GoogleTest)"));
        assert!(output.contains("add_custom_target(TestProject_tests)"));
        assert!(output.contains("C3PG_GTEST_MAIN"));
    }

    #[test]
    fn test_sanitize_to_c_identifier() {
        assert_eq!(sanitize_to_c_identifier("test_math"), "test_math");
        assert_eq!(sanitize_to_c_identifier("test-utils"), "test_utils");
        assert_eq!(sanitize_to_c_identifier("3test"), "_3test");
        assert_eq!(sanitize_to_c_identifier("hello world"), "hello_world");
    }
}
