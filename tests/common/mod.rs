use c3pg::config::{Config, TargetConfig, TargetType};
use c3pg::test_utils::MockCommandRunner;
use c3pg::traits::{FromFile, ToFile};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Conan remote list output used by default in mocks.
pub const CONAN_REMOTE_LIST: &str = "conancenter: https://center2.conan.io [Enabled]\n";

/// Conan search result for gtest.
pub const CONAN_SEARCH_GTEST: &str = "gtest/1.14.0\ngtest/1.15.0\n";

/// Conan search result for fmt.
pub const CONAN_SEARCH_FMT: &str = "fmt/10.1.0\nfmt/10.2.0\nfmt/11.0.0\n";

/// Create a `MockCommandRunner` pre-loaded with standard conan remote and
/// gtest search responses (sufficient for `cmd_new` and basic `cmd_add`).
pub fn mock_with_conan_responses() -> MockCommandRunner {
    let runner = MockCommandRunner::new(Some(String::new()));
    runner.on_success("conan", &["remote", "list"], CONAN_REMOTE_LIST);
    runner.on_success("conan", &["search", "gtest"], CONAN_SEARCH_GTEST);
    runner.on_success("conan", &["search", "fmt"], CONAN_SEARCH_FMT);
    runner
}

/// Set up a temporary directory that looks like an existing c3pg project.
/// Returns the `TempDir` handle (directory is deleted on drop) and the config.
///
/// The directory contains:
/// - `c3pg.toml`
/// - `src/main.cpp`
/// - `build/CMakeLists.txt`
/// - `build/conanfile.py`
pub fn setup_project_dir(name: &str) -> (TempDir, Config) {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let root = tmp.path();

    // Create directory structure
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("build")).unwrap();

    // Write a simple main.cpp
    fs::write(
        root.join("src/main.cpp"),
        r#"#include <iostream>
int main() {
    std::cout << "Hello from C3PG!" << std::endl;
    return 0;
}
"#,
    )
    .unwrap();

    // Build a config with a default executable target
    let mut config = Config::default();
    config.project.name = name.to_string();
    config.targets = vec![TargetConfig {
        name: name.to_string(),
        target_type: TargetType::Executable,
        sources: vec!["src/main.cpp".to_string()],
        public_include: vec![],
        link: vec![],
    }];

    // Write config
    config.to_file(root.join("c3pg.toml")).unwrap();

    // Write placeholder generated files
    fs::write(root.join("build/CMakeLists.txt"), "# placeholder\n").unwrap();
    fs::write(root.join("build/conanfile.py"), "# placeholder\n").unwrap();

    (tmp, config)
}

/// Read a config back from the project directory.
pub fn read_config(project_root: &Path) -> Config {
    Config::from_file(project_root.join("c3pg.toml")).expect("failed to read c3pg.toml")
}

/// Set up a temporary directory that looks like a multi-target c3pg project.
/// Returns the `TempDir` handle and the config.
///
/// Layout:
/// - `src/lib/math.cpp`
/// - `src/main.cpp`
/// - `src/tool/tool.cpp`
/// - `include/` (empty, for public headers)
/// - `build/CMakeLists.txt`
/// - `build/conanfile.py`
/// - `c3pg.toml` with `[[targets]]`
pub fn setup_multitarget_project(name: &str) -> (TempDir, Config) {
    use c3pg::config::{TargetConfig, TargetType};

    let tmp = TempDir::new().expect("failed to create temp dir");
    let root = tmp.path();

    // Create directory structure
    fs::create_dir_all(root.join("src/lib")).unwrap();
    fs::create_dir_all(root.join("src/tool")).unwrap();
    fs::create_dir_all(root.join("include")).unwrap();
    fs::create_dir_all(root.join("build")).unwrap();

    // Write source files
    fs::write(root.join("src/lib/math.cpp"), "int add(int a, int b) { return a + b; }\n").unwrap();
    fs::write(root.join("src/main.cpp"), "int main() { return 0; }\n").unwrap();
    fs::write(root.join("src/tool/tool.cpp"), "int main() { return 0; }\n").unwrap();

    let mut config = Config::default();
    config.project.name = name.to_string();
    config.targets = vec![
        TargetConfig {
            name: "mylib".into(),
            target_type: TargetType::StaticLibrary,
            sources: vec!["src/lib".into()],
            public_include: vec!["include".into()],
            link: vec![],
        },
        TargetConfig {
            name: "myapp".into(),
            target_type: TargetType::Executable,
            sources: vec!["src/main.cpp".into()],
            public_include: vec![],
            link: vec!["mylib".into()],
        },
        TargetConfig {
            name: "mytool".into(),
            target_type: TargetType::Executable,
            sources: vec!["src/tool".into()],
            public_include: vec![],
            link: vec!["mylib".into()],
        },
    ];

    config.to_file(root.join("c3pg.toml")).unwrap();

    // Write placeholder generated files
    fs::write(root.join("build/CMakeLists.txt"), "# placeholder\n").unwrap();
    fs::write(root.join("build/conanfile.py"), "# placeholder\n").unwrap();

    (tmp, config)
}

/// Assert a file exists at the given path.
pub fn assert_file_exists(path: &Path) {
    assert!(
        path.exists(),
        "Expected file to exist: {}",
        path.display()
    );
}

/// Assert a file exists and contains the given substring.
pub fn assert_file_contains(path: &Path, needle: &str) {
    assert_file_exists(path);
    let contents = fs::read_to_string(path).unwrap_or_else(|e| {
        panic!("Failed to read {}: {e}", path.display());
    });
    assert!(
        contents.contains(needle),
        "Expected {} to contain {needle:?}, but contents were:\n{contents}",
        path.display()
    );
}

/// Assert a file exists and does NOT contain the given substring.
pub fn assert_file_not_contains(path: &Path, needle: &str) {
    assert_file_exists(path);
    let contents = fs::read_to_string(path).unwrap_or_else(|e| {
        panic!("Failed to read {}: {e}", path.display());
    });
    assert!(
        !contents.contains(needle),
        "Expected {} NOT to contain {needle:?}, but it did.\nContents:\n{contents}",
        path.display()
    );
}
