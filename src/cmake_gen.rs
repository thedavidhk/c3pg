use std::path::Path;

use crate::{
    cmake_core::{
        DiscoverMode, LibType, Project,
        Scope::{PRIVATE, PUBLIC},
        Target, TestEntry, TestFramework, TestSuite, Value,
    },
    config::{Config, EffectiveTarget, TargetConfig, TargetType},
};
use anyhow::Result;
use walkdir::WalkDir;

/// Generate a complete `CMakeLists.txt` string from the project configuration.
///
/// Source files are discovered from the `src/` and test directories on disk.
/// If `[[targets]]` are declared in the config they are used; otherwise the
/// current auto-detect convention (single exe + optional lib) is applied.
///
/// # Errors
///
/// Returns an error if target resolution fails or the underlying
/// [`Project::emit`](crate::cmake_core::Project::emit) call fails.
pub fn generate_cmakelists(config: &Config) -> Result<String> {
    let project_name = &config.project.name;
    let std_str = config.project.standard.to_string();

    // Compute the relative path from cache_dir back to the project root.
    let project_root_ref = cmake_project_root_ref(&config.project.cache_dir);

    let targets = effective_targets(config)?;

    // Collect library target names for test linking.
    let lib_names: Vec<String> = targets
        .iter()
        .filter(|t| t.target_type == TargetType::StaticLibrary)
        .map(|t| t.name.clone())
        .collect();

    let mut project = Project::new(project_name)
        .lang(&["CXX"])
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

    // Emit each effective target.
    for et in &targets {
        let sources: Vec<Value> = et
            .source_files
            .iter()
            .map(|p| project_root_value(&project_root_ref, p))
            .collect();

        let cmake_target = match et.target_type {
            TargetType::StaticLibrary => {
                let mut t = Target::library(&et.name, LibType::Static).srcs(sources);
                // Public include directories.
                for inc in &et.public_include {
                    t = t.include(PUBLIC, project_root_value(&project_root_ref, inc));
                }
                // Libraries get PUBLIC Conan deps so consumers inherit them.
                t = t.link(PUBLIC, Value::Raw("${CONANDEPS_LEGACY}".into()));
                // Internal link dependencies.
                for dep in &et.link {
                    t = t.link(PUBLIC, dep.as_str());
                }
                t
            }
            TargetType::Executable => {
                let mut t = Target::executable(&et.name).srcs(sources);
                if et.link.is_empty() {
                    // No internal deps: link Conan deps directly.
                    t = t.link(PRIVATE, Value::Raw("${CONANDEPS_LEGACY}".into()));
                } else {
                    // Internal deps: link those targets (they propagate
                    // Conan deps transitively via PUBLIC linkage).
                    for dep in &et.link {
                        t = t.link(PRIVATE, dep.as_str());
                    }
                }
                t
            }
        };

        project = project.target(cmake_target);
    }

    if let Some(suite) = build_test_suite(config, &lib_names, &project_root_ref, &std_str) {
        project = project.with_tests(suite);
    }

    project.emit()
}

/// Build a `TestSuite` from test source files discovered on disk.
///
/// Returns `None` when no test files exist in the configured test directory.
fn build_test_suite(
    config: &Config,
    lib_names: &[String],
    project_root_ref: &str,
    std_str: &str,
) -> Option<TestSuite> {
    let test_files = find_files(&config.testing.dir, &SOURCE_EXTENSIONS);
    if test_files.is_empty() {
        return None;
    }

    let gtest = TestFramework::GoogleTest {
        config_mode: true,
        inline_main_var: "C3PG_GTEST_MAIN".into(),
        inline_main_body: GTEST_MAIN_BODY.trim().into(),
        discover_mode: DiscoverMode::PreTest,
    };

    let cxx_std: u16 = std_str.parse().unwrap_or(20);

    // In convention mode (no library targets), compile project sources
    // (minus main.cpp) directly into each test binary so tests can
    // access the project's code without a separate library target.
    let convention_src_paths: Vec<String> = if lib_names.is_empty() {
        find_files("src", &SOURCE_EXTENSIONS)
            .into_iter()
            .filter(|p| !p.ends_with("main.cpp"))
            .collect()
    } else {
        vec![]
    };

    let entries = test_files.iter().map(|path| {
        let base = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("test");
        let safe_name = sanitize_to_c_identifier(base);
        let mut link: Vec<Value> = lib_names
            .iter()
            .map(|n| Value::Str(n.clone()))
            .collect();
        if link.is_empty() {
            link.push(Value::Raw("${CONANDEPS_LEGACY}".into()));
        }
        link.push(Value::Raw("gtest::gtest".into()));
        let mut sources = vec![
            project_root_value(project_root_ref, path),
            Value::Raw("${C3PG_GTEST_MAIN}".into()),
        ];
        for p in &convention_src_paths {
            sources.push(project_root_value(project_root_ref, p));
        }
        TestEntry {
            exe_name: safe_name.clone(),
            sources,
            link,
            prefix: format!("{base}."),
            cxx_standard: Some(cxx_std),
        }
    });

    let tests_target = format!("{}_tests", config.project.name);
    let suite = entries.fold(
        TestSuite::new_aggregate(&tests_target, gtest),
        super::cmake_core::TestSuite::with_entry,
    );

    Some(suite)
}

const GTEST_MAIN_BODY: &str = r"
#include <gtest/gtest.h>
int main(int argc, char** argv) {
  ::testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}
";

pub const SOURCE_EXTENSIONS: [&str; 4] = ["c", "cpp", "cxx", "cc"];

/// Header file extensions recognised by `c3pg fmt` and `c3pg lint`.
pub const HEADER_EXTENSIONS: [&str; 4] = ["h", "hpp", "hxx", "hh"];

/// Compute the `${CMAKE_CURRENT_LIST_DIR}/..` prefix that navigates from
/// `cache_dir` back to the project root.  For `cache_dir = "build"` this
/// returns `"${CMAKE_CURRENT_LIST_DIR}/.."`, for `"build/debug"` it returns
/// `"${CMAKE_CURRENT_LIST_DIR}/../.."`, etc.
fn cmake_project_root_ref(cache_dir: &str) -> String {
    let depth = Path::new(cache_dir).components().count();
    let ups = std::iter::repeat_n("..", depth).collect::<Vec<_>>().join("/");
    format!("${{CMAKE_CURRENT_LIST_DIR}}/{ups}")
}

/// Wrap a project-root-relative path using the computed root reference.
///
/// The generated `CMakeLists.txt` lives inside the cache/build directory, so all
/// references to project files need this prefix to resolve correctly.
fn project_root_value(project_root_ref: &str, path: &str) -> Value {
    Value::Raw(format!("{project_root_ref}/{path}"))
}

/// Walk a directory tree and return paths (relative to CWD) of files whose
/// extension matches one of `exts`. Returns an empty vec if the directory
/// does not exist or is unreadable.
pub fn find_files(root: &str, exts: &[&str]) -> Vec<String> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| exts.contains(&ext))
        })
        .map(|e| e.into_path().to_string_lossy().into_owned())
        .collect()
}

/// Resolve a `sources` entry to concrete file paths.
///
/// If the entry is a directory, it is walked recursively for C/C++ source
/// files. Otherwise it is treated as an explicit file path and returned
/// as-is (it may not exist yet, e.g. during scaffolding -- cmake will
/// report the error at build time).
fn resolve_source_entry(entry: &str) -> Vec<String> {
    let path = Path::new(entry);
    if path.is_dir() {
        find_files(entry, &SOURCE_EXTENSIONS)
    } else {
        vec![entry.to_string()]
    }
}

/// Resolve explicit `[[targets]]` into [`EffectiveTarget`]s by expanding
/// directory entries to concrete source file paths.
///
/// # Errors
///
/// Returns an error if target validation fails (duplicate names, cycles, etc.).
pub fn resolve_targets(targets: &[TargetConfig]) -> Result<Vec<EffectiveTarget>> {
    crate::config::validate_targets(targets)?;

    targets
        .iter()
        .map(|tc| {
            let mut source_files: Vec<String> = tc
                .sources
                .iter()
                .flat_map(|entry| resolve_source_entry(entry))
                .collect();
            source_files.sort();
            source_files.dedup();
            Ok(EffectiveTarget {
                name: tc.name.clone(),
                target_type: tc.target_type.clone(),
                source_files,
                public_include: tc.public_include.clone(),
                link: tc.link.clone(),
            })
        })
        .collect()
}

/// Convention-based target inference when no `[[targets]]` are declared.
///
/// All source files in `src/` are compiled into a single executable named
/// after the project.  If no sources are found yet (e.g. during scaffolding),
/// a conventional `src/main.cpp` entry is used as a placeholder.
fn convention_targets(project_name: &str) -> Vec<EffectiveTarget> {
    let all_sources = find_files("src", &SOURCE_EXTENSIONS);
    let source_files = if all_sources.is_empty() {
        vec!["src/main.cpp".to_string()]
    } else {
        all_sources
    };
    vec![EffectiveTarget {
        name: project_name.to_string(),
        target_type: TargetType::Executable,
        source_files,
        public_include: vec![],
        link: vec![],
    }]
}

/// Return the effective targets for a config.
///
/// When no `[[targets]]` are declared, convention-based inference is used
/// (all sources in `src/` become a single executable).  When `[[targets]]`
/// are present, they take full control.
///
/// # Errors
///
/// Returns an error if explicit targets fail validation or source resolution.
pub fn effective_targets(config: &Config) -> Result<Vec<EffectiveTarget>> {
    if config.targets.is_empty() {
        Ok(convention_targets(&config.project.name))
    } else {
        resolve_targets(&config.targets)
    }
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
        .is_some_and(|c| c.is_ascii_digit())
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

    fn test_config(name: &str, standard: CppStandard) -> Config {
        Config {
            project: crate::config::Project {
                name: name.to_string(),
                standard,
                cache_dir: "build".to_string(),
            },
            dependencies: std::collections::BTreeMap::new(),
            targets: vec![],
            cmake: CMakeConfig::default(),
            conan: ConanConfig::default(),
            testing: TestingConfig::default(),
        }
    }

    /// Run a closure with CWD set to a temp dir that has `src/main.cpp`.
    fn with_src_main<F, R>(f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/main.cpp"), "int main() {}\n").unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = f();
        let _ = std::env::set_current_dir(prev);
        result
    }

    #[test]
    fn test_generate_basic_project() {
        with_src_main(|| {
            let config = test_config("MyProject", CppStandard::Cpp20);
            let output = generate_cmakelists(&config).unwrap();

            assert!(output.contains("cmake_minimum_required(VERSION 3.21)"));
            assert!(output.contains("project(MyProject LANGUAGES CXX)"));
            assert!(output.contains("set(CMAKE_CXX_STANDARD 20)"));
            assert!(output.contains("set(CMAKE_CXX_STANDARD_REQUIRED ON)"));
            assert!(output.contains("set(CMAKE_EXPORT_COMPILE_COMMANDS ON)"));
            assert!(output.contains("include(${CMAKE_BINARY_DIR}/conandeps_legacy.cmake)"));
            assert!(output.contains("add_executable(MyProject"));
            assert!(output.contains("src/main.cpp"));

            // Single executable: no library
            assert!(!output.contains("add_library("));
            assert!(output.contains("${CONANDEPS_LEGACY}"));

            // No test section when no test files exist
            assert!(!output.contains("include(CTest)"));
            assert!(!output.contains("enable_testing()"));
        });
    }

    #[test]
    fn test_generate_with_cpp17_and_no_export() {
        with_src_main(|| {
            let mut config = test_config("NoDepsProject", CppStandard::Cpp17);
            config.cmake.export_compile_commands = false;
            let output = generate_cmakelists(&config).unwrap();

            assert!(output.contains("set(CMAKE_CXX_STANDARD 17)"));
            assert!(output.contains("set(CMAKE_EXPORT_COMPILE_COMMANDS OFF)"));
            assert!(output.contains("project(NoDepsProject LANGUAGES CXX)"));
        });
    }

    #[test]
    fn test_cmake_project_root_ref() {
        assert_eq!(
            cmake_project_root_ref("build"),
            "${CMAKE_CURRENT_LIST_DIR}/.."
        );
        assert_eq!(
            cmake_project_root_ref("build/debug"),
            "${CMAKE_CURRENT_LIST_DIR}/../.."
        );
    }

    #[test]
    fn test_sanitize_to_c_identifier() {
        assert_eq!(sanitize_to_c_identifier("test_math"), "test_math");
        assert_eq!(sanitize_to_c_identifier("test-utils"), "test_utils");
        assert_eq!(sanitize_to_c_identifier("3test"), "_3test");
        assert_eq!(sanitize_to_c_identifier("hello world"), "hello_world");
    }

    // -----------------------------------------------------------------------
    // Source resolution / auto-detect tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_multitarget_project() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Create source layout
        let lib_dir = tmp.path().join("src/lib");
        std::fs::create_dir_all(&lib_dir).unwrap();
        std::fs::write(lib_dir.join("math.cpp"), "").unwrap();
        std::fs::write(tmp.path().join("src/main.cpp"), "").unwrap();
        let tool_dir = tmp.path().join("src/tool");
        std::fs::create_dir_all(&tool_dir).unwrap();
        std::fs::write(tool_dir.join("tool.cpp"), "").unwrap();
        // Create include dir for the assert
        std::fs::create_dir_all(tmp.path().join("include")).unwrap();

        let config = Config {
            project: crate::config::Project {
                name: "multi".to_string(),
                standard: CppStandard::Cpp20,
                cache_dir: "build".to_string(),
            },
            dependencies: std::collections::BTreeMap::new(),
            targets: vec![
                crate::config::TargetConfig {
                    name: "mylib".into(),
                    target_type: crate::config::TargetType::StaticLibrary,
                    sources: vec![lib_dir.to_str().unwrap().to_string()],
                    public_include: vec!["include".into()],
                    link: vec![],
                },
                crate::config::TargetConfig {
                    name: "myapp".into(),
                    target_type: crate::config::TargetType::Executable,
                    sources: vec![
                        tmp.path().join("src/main.cpp").to_str().unwrap().to_string(),
                    ],
                    public_include: vec![],
                    link: vec!["mylib".into()],
                },
                crate::config::TargetConfig {
                    name: "mytool".into(),
                    target_type: crate::config::TargetType::Executable,
                    sources: vec![tool_dir.to_str().unwrap().to_string()],
                    public_include: vec![],
                    link: vec!["mylib".into()],
                },
            ],
            cmake: CMakeConfig::default(),
            conan: ConanConfig::default(),
            testing: TestingConfig::default(),
        };

        let output = generate_cmakelists(&config).unwrap();

        // Should have all three targets
        assert!(
            output.contains("add_library(mylib STATIC"),
            "Expected static library, got:\n{output}"
        );
        assert!(
            output.contains("add_executable(myapp"),
            "Expected myapp executable, got:\n{output}"
        );
        assert!(
            output.contains("add_executable(mytool"),
            "Expected mytool executable, got:\n{output}"
        );

        // mylib should have PUBLIC include dir
        assert!(
            output.contains("include") && output.contains("PUBLIC"),
            "Library should have PUBLIC include"
        );

        // myapp should link to mylib
        assert!(
            output.contains("target_link_libraries(myapp"),
            "myapp should link libraries"
        );

        // mytool should link to mylib
        assert!(
            output.contains("target_link_libraries(mytool"),
            "mytool should link libraries"
        );
    }

    #[test]
    fn test_resolve_source_entry_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let f = tmp.path().join("hello.cpp");
        std::fs::write(&f, "int main() {}").unwrap();
        let result = resolve_source_entry(f.to_str().unwrap());
        assert_eq!(result, vec![f.to_str().unwrap().to_string()]);
    }

    #[test]
    fn test_resolve_source_entry_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.cpp"), "").unwrap();
        std::fs::write(src.join("b.hpp"), "").unwrap(); // header, should be skipped
        std::fs::write(src.join("c.cc"), "").unwrap();
        let result = resolve_source_entry(src.to_str().unwrap());
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|p| p.contains("a.cpp")));
        assert!(result.iter().any(|p| p.contains("c.cc")));
    }

    #[test]
    fn test_resolve_source_entry_nonexistent() {
        // Non-existent paths are returned as-is (CMake will error at build time).
        let result = resolve_source_entry("/nonexistent/path");
        assert_eq!(result, vec!["/nonexistent/path"]);
    }

    #[test]
    fn test_resolve_targets_valid() {
        use crate::config::{TargetConfig, TargetType};

        let tmp = tempfile::TempDir::new().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("lib.cpp"), "").unwrap();
        let main_cpp = tmp.path().join("main.cpp");
        std::fs::write(&main_cpp, "int main() {}").unwrap();

        let targets = vec![
            TargetConfig {
                name: "mylib".into(),
                target_type: TargetType::StaticLibrary,
                sources: vec![src.to_str().unwrap().to_string()],
                public_include: vec!["include".into()],
                link: vec![],
            },
            TargetConfig {
                name: "myapp".into(),
                target_type: TargetType::Executable,
                sources: vec![main_cpp.to_str().unwrap().to_string()],
                public_include: vec![],
                link: vec!["mylib".into()],
            },
        ];

        let effective = resolve_targets(&targets).unwrap();
        assert_eq!(effective.len(), 2);
        assert_eq!(effective[0].name, "mylib");
        assert_eq!(effective[0].source_files.len(), 1);
        assert!(effective[0].source_files[0].contains("lib.cpp"));
        assert_eq!(effective[1].name, "myapp");
        assert_eq!(effective[1].link, vec!["mylib"]);
    }
}
