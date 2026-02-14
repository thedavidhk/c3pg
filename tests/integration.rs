mod common;

use std::fs;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::Mutex;

use c3pg::cli::{TestArgs, TestOnlySubcmds};
use c3pg::cmake::{BuildType, CppStandard, Sanitizers};
use c3pg::commands::*;
use c3pg::config::DependencyValue;
use c3pg::test_utils::MockCommandRunner;
use c3pg::traits::FromFile;
use log::LevelFilter;
use tempfile::TempDir;

use common::*;

// Tests that change the process CWD must be serialised.
static CWD_LOCK: Mutex<()> = Mutex::new(());

/// Run `f` with the process CWD temporarily set to `dir`.
/// The original CWD is always restored, even on panic.
fn with_cwd<F, R>(dir: &Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = CWD_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir).unwrap();
    let result = std::panic::catch_unwind(AssertUnwindSafe(f));
    let _ = std::env::set_current_dir(original);
    result.unwrap_or_else(|e| std::panic::resume_unwind(e))
}

// ---------------------------------------------------------------------------
// cmd_new tests
// ---------------------------------------------------------------------------

#[test]
fn test_new_creates_project_structure() {
    let tmp = TempDir::new().unwrap();
    let runner = mock_with_conan_responses();

    with_cwd(tmp.path(), || {
        cmd_new(&runner, "myproject", false, CppStandard::default()).unwrap();
    });

    let root = tmp.path().join("myproject");
    assert_file_exists(&root.join("src/main.cpp"));
    assert_file_exists(&root.join("build/CMakeLists.txt"));
    assert_file_exists(&root.join("build/conanfile.py"));
    assert_file_exists(&root.join("c3pg.toml"));

    // main.cpp should have the hello world content
    assert_file_contains(&root.join("src/main.cpp"), "Hello from C3PG!");
}

#[test]
fn test_new_with_no_git() {
    let tmp = TempDir::new().unwrap();
    let runner = mock_with_conan_responses();

    with_cwd(tmp.path(), || {
        cmd_new(&runner, "nogit", false, CppStandard::default()).unwrap();
    });

    let root = tmp.path().join("nogit");
    assert!(!root.join(".gitignore").exists());
    runner.assert_did_not_run("git");
}

#[test]
fn test_new_with_git() {
    let tmp = TempDir::new().unwrap();
    let runner = mock_with_conan_responses();
    // git init needs to succeed in the mock
    runner.on_success("git", &["init"], "Initialized empty Git repository");

    with_cwd(tmp.path(), || {
        cmd_new(&runner, "withgit", true, CppStandard::default()).unwrap();
    });

    let root = tmp.path().join("withgit");
    assert_file_exists(&root.join(".gitignore"));
    assert_file_contains(&root.join(".gitignore"), "build");
    runner.assert_ran("git", &["init", "-b", "main", "withgit"]);
}

#[test]
fn test_new_has_no_gtest_dependency() {
    let tmp = TempDir::new().unwrap();
    let runner = mock_with_conan_responses();

    with_cwd(tmp.path(), || {
        cmd_new(&runner, "testgtest", false, CppStandard::default()).unwrap();
    });

    let root = tmp.path().join("testgtest");

    // Config should NOT contain gtest (it's added lazily via `test add`)
    let config = c3pg::config::Config::from_file(root.join("c3pg.toml")).unwrap();
    assert!(
        !config.has_dependency("gtest"),
        "Expected no gtest in dependencies for a fresh project, got: {:?}",
        config.dependencies
    );

    // conanfile.py should not have a gtest requires line
    assert_file_not_contains(&root.join("build/conanfile.py"), "self.requires(\"gtest");
}

#[test]
fn test_new_with_cpp17_standard() {
    let tmp = TempDir::new().unwrap();
    let runner = mock_with_conan_responses();

    with_cwd(tmp.path(), || {
        cmd_new(&runner, "cpp17proj", false, CppStandard::Cpp17).unwrap();
    });

    let root = tmp.path().join("cpp17proj");

    // Config should have Cpp17
    let config = c3pg::config::Config::from_file(root.join("c3pg.toml")).unwrap();
    assert_eq!(config.project.standard, CppStandard::Cpp17);

    // CMakeLists.txt should reference standard 17
    assert_file_contains(&root.join("build/CMakeLists.txt"), "CMAKE_CXX_STANDARD 17");
}

#[test]
fn test_new_fails_gracefully_if_conan_has_no_remotes() {
    let tmp = TempDir::new().unwrap();
    let runner = MockCommandRunner::new(Some(String::new()));
    // conan remote list returns empty output => get_first_remote fails
    runner.on_success("conan", &["remote", "list"], "");

    let result = with_cwd(tmp.path(), || {
        cmd_new(&runner, "failproj", false, CppStandard::default())
    });

    assert!(
        result.is_err(),
        "Expected cmd_new to fail when conan has no remotes"
    );
    let err_msg = format!("{:#}", result.unwrap_err());
    assert!(
        err_msg.contains("remote") || err_msg.contains("empty"),
        "Error message should mention remotes, got: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// cmd_init tests
// ---------------------------------------------------------------------------

#[test]
fn test_init_creates_project_in_current_dir() {
    let tmp = TempDir::new().unwrap();
    let runner = mock_with_conan_responses();

    with_cwd(tmp.path(), || {
        cmd_init(&runner, false, CppStandard::default()).unwrap();
    });

    assert_file_exists(&tmp.path().join("c3pg.toml"));
    assert_file_exists(&tmp.path().join("src/main.cpp"));
    assert_file_exists(&tmp.path().join("build/CMakeLists.txt"));
    assert_file_exists(&tmp.path().join("build/conanfile.py"));
    assert_file_contains(&tmp.path().join("src/main.cpp"), "Hello from C3PG!");
}

#[test]
fn test_init_preserves_existing_sources() {
    let tmp = TempDir::new().unwrap();
    let runner = mock_with_conan_responses();

    // Pre-create src/ with a custom file
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(tmp.path().join("src/app.cpp"), "// my app\n").unwrap();

    with_cwd(tmp.path(), || {
        cmd_init(&runner, false, CppStandard::default()).unwrap();
    });

    // Custom source should still be there
    assert_file_contains(&tmp.path().join("src/app.cpp"), "// my app");
    // main.cpp should NOT have been written (sources already exist)
    assert!(!tmp.path().join("src/main.cpp").exists());
    // But config should still be created
    assert_file_exists(&tmp.path().join("c3pg.toml"));
}

#[test]
fn test_init_fails_if_already_initialized() {
    let tmp = TempDir::new().unwrap();
    let runner = mock_with_conan_responses();

    // First init succeeds
    with_cwd(tmp.path(), || {
        cmd_init(&runner, false, CppStandard::default()).unwrap();
    });

    // Second init should fail
    let result = with_cwd(tmp.path(), || {
        cmd_init(&runner, false, CppStandard::default())
    });
    assert!(result.is_err());
    let err_msg = format!("{:#}", result.unwrap_err());
    assert!(
        err_msg.contains("already initialized"),
        "Error should mention already initialized, got: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// cmd_add / cmd_remove tests
// ---------------------------------------------------------------------------

#[test]
fn test_add_dependency() {
    let (tmp, _config) = setup_project_dir("addtest");
    let runner = mock_with_conan_responses();

    with_cwd(tmp.path(), || {
        cmd_add(&runner, "fmt").unwrap();
    });

    // Config should now contain fmt
    let config = read_config(tmp.path());
    assert!(
        config.has_dependency("fmt"),
        "Expected fmt in dependencies, got: {:?}",
        config.dependencies
    );

    // conanfile.py should reference fmt
    assert_file_contains(
        &tmp.path().join("build/conanfile.py"),
        "self.requires(\"fmt/11.0.0\")",
    );

    // CMakeLists.txt should have been regenerated (non-placeholder)
    assert_file_contains(
        &tmp.path().join("build/CMakeLists.txt"),
        "cmake_minimum_required",
    );

    // Should have searched for fmt
    runner.assert_ran("conan", &["search", "fmt"]);
}

#[test]
fn test_add_dependency_replaces_existing() {
    let (tmp, _config) = setup_project_dir("replacetest");

    // First add: fmt/11.0.0
    let runner1 = mock_with_conan_responses();
    with_cwd(tmp.path(), || {
        cmd_add(&runner1, "fmt").unwrap();
    });

    let config1 = read_config(tmp.path());
    assert_eq!(
        config1.dependencies["fmt"],
        DependencyValue::Simple("11.0.0".to_string())
    );

    // Second add with a mock that returns a different set of versions.
    // Use a fresh mock so the fmt search override takes precedence.
    let runner2 = MockCommandRunner::new(Some(String::new()));
    runner2.on_success("conan", &["remote", "list"], CONAN_REMOTE_LIST);
    runner2.on_success("conan", &["search", "fmt"], "fmt/10.0.0\nfmt/10.2.0\n");
    with_cwd(tmp.path(), || {
        cmd_add(&runner2, "fmt").unwrap();
    });

    let config2 = read_config(tmp.path());
    assert_eq!(
        config2.dependencies["fmt"],
        DependencyValue::Simple("10.2.0".to_string())
    );

    // Should still only have one fmt entry
    assert_eq!(
        config2
            .dependencies
            .keys()
            .filter(|k| k.as_str() == "fmt")
            .count(),
        1
    );
}

#[test]
fn test_remove_dependency() {
    let (tmp, _) = setup_project_dir("removetest");

    // First add a dependency
    let runner = mock_with_conan_responses();
    with_cwd(tmp.path(), || {
        cmd_add(&runner, "fmt").unwrap();
    });

    // Verify it's there
    let config = read_config(tmp.path());
    assert!(config.has_dependency("fmt"));

    // Now remove it
    let runner2 = mock_with_conan_responses();
    with_cwd(tmp.path(), || {
        cmd_remove(&runner2, "fmt").unwrap();
    });

    // Should be gone from config
    let config2 = read_config(tmp.path());
    assert!(
        !config2.has_dependency("fmt"),
        "fmt should have been removed, got: {:?}",
        config2.dependencies
    );

    // conanfile.py should no longer mention fmt
    assert_file_not_contains(
        &tmp.path().join("build/conanfile.py"),
        "self.requires(\"fmt",
    );
}

#[test]
fn test_remove_nonexistent_dependency() {
    let (tmp, _) = setup_project_dir("removefail");
    let runner = mock_with_conan_responses();

    let result = with_cwd(tmp.path(), || cmd_remove(&runner, "nonexistent"));

    assert!(result.is_err());
    let err_msg = format!("{:#}", result.unwrap_err());
    assert!(
        err_msg.contains("not found"),
        "Error should mention 'not found', got: {err_msg}"
    );
}

// ---------------------------------------------------------------------------
// cmd_build / cmd_run tests
// ---------------------------------------------------------------------------

#[test]
fn test_build_invokes_conan_install_then_cmake() {
    let (tmp, _) = setup_project_dir("buildtest");
    let runner = mock_with_conan_responses();

    with_cwd(tmp.path(), || {
        cmd_build(
            &runner,
            BuildType::Debug,
            LevelFilter::Info,
            &Sanitizers::default(),
        )
        .unwrap();
    });

    // Should have run conan install
    runner.assert_ran("conan", &["install"]);

    // Should have run cmake configure (-B)
    runner.assert_ran("cmake", &["-B"]);

    // Should have run cmake build (--build)
    runner.assert_ran("cmake", &["--build"]);

    // Verify command order: conan before cmake
    let cmds = runner.executed_commands();
    let conan_idx = cmds.iter().position(|(c, _)| c == "conan").unwrap();
    let cmake_configure_idx = cmds
        .iter()
        .position(|(c, a)| c == "cmake" && a.contains(&"-B".to_string()))
        .unwrap();
    let cmake_build_idx = cmds
        .iter()
        .position(|(c, a)| c == "cmake" && a.contains(&"--build".to_string()))
        .unwrap();

    assert!(
        conan_idx < cmake_configure_idx,
        "conan install should run before cmake configure"
    );
    assert!(
        cmake_configure_idx < cmake_build_idx,
        "cmake configure should run before cmake build"
    );
}

#[test]
fn test_build_passes_build_type() {
    let (tmp, _) = setup_project_dir("buildrelease");
    let runner = mock_with_conan_responses();

    with_cwd(tmp.path(), || {
        cmd_build(
            &runner,
            BuildType::Release,
            LevelFilter::Info,
            &Sanitizers::default(),
        )
        .unwrap();
    });

    // Conan should receive build_type=Release
    runner.assert_ran("conan", &["install", "build_type=Release"]);

    // CMake should receive -DCMAKE_BUILD_TYPE=Release
    runner.assert_ran("cmake", &["-DCMAKE_BUILD_TYPE=Release"]);
}

#[test]
fn test_run_builds_then_executes_binary() {
    let (tmp, _) = setup_project_dir("runtest");
    let runner = mock_with_conan_responses();
    // The binary will be at build/<project_name>
    runner.on_success("build/runtest", &[], "Hello from C3PG!");

    with_cwd(tmp.path(), || {
        cmd_run(
            &runner,
            BuildType::Debug,
            LevelFilter::Info,
            &Sanitizers::default(),
            None,
        )
        .unwrap();
    });

    // Build commands should have run
    runner.assert_ran("conan", &["install"]);
    runner.assert_ran("cmake", &["--build"]);

    // Binary should have been executed
    runner.assert_ran("build/runtest", &[]);
}

// ---------------------------------------------------------------------------
// cmd_test tests
// ---------------------------------------------------------------------------

#[test]
fn test_test_add_creates_file() {
    let (tmp, _) = setup_project_dir("testadd");
    let runner = mock_with_conan_responses();

    let args = TestArgs {
        filter: None,
        jobs: None,
        sanitizers: Sanitizers::default(),
        command: Some(TestOnlySubcmds::Add {
            name: "math".to_string(),
        }),
    };

    with_cwd(tmp.path(), || {
        cmd_test(&runner, args, LevelFilter::Info).unwrap();
    });

    // Should have created tests/test_math.cpp (and the tests/ directory)
    let test_file = tmp.path().join("tests/test_math.cpp");
    assert_file_exists(&test_file);
    assert_file_contains(&test_file, "#include <gtest/gtest.h>");
    assert_file_contains(&test_file, "TEST(math, hello_test)");

    // Gtest should have been lazily added to dependencies
    let config = read_config(tmp.path());
    assert!(
        config.has_dependency("gtest"),
        "Expected gtest in dependencies after test add, got: {:?}",
        config.dependencies
    );
}

#[test]
fn test_test_add_skips_existing() {
    let (tmp, _) = setup_project_dir("testskip");
    let runner = mock_with_conan_responses();

    // Pre-create the test file with custom content (dir + file)
    fs::create_dir_all(tmp.path().join("tests")).unwrap();
    let test_file = tmp.path().join("tests/test_existing.cpp");
    fs::write(&test_file, "// my custom test\n").unwrap();

    let args = TestArgs {
        filter: None,
        jobs: None,
        sanitizers: Sanitizers::default(),
        command: Some(TestOnlySubcmds::Add {
            name: "existing".to_string(),
        }),
    };

    with_cwd(tmp.path(), || {
        cmd_test(&runner, args, LevelFilter::Info).unwrap();
    });

    // File should still have the original content (not overwritten)
    assert_file_contains(&test_file, "// my custom test");
    assert_file_not_contains(&test_file, "#include <gtest/gtest.h>");
}

#[test]
fn test_test_run_invokes_cmake_and_ctest() {
    let (tmp, _) = setup_project_dir("testrun");
    let runner = mock_with_conan_responses();

    // Create a test file so auto-detection finds it
    fs::create_dir_all(tmp.path().join("tests")).unwrap();
    fs::write(tmp.path().join("tests/test_hello.cpp"), "// test\n").unwrap();

    let args = TestArgs {
        filter: None,
        jobs: None,
        sanitizers: Sanitizers::default(),
        command: None, // no subcommand => run tests
    };

    with_cwd(tmp.path(), || {
        cmd_test(&runner, args, LevelFilter::Info).unwrap();
    });

    // Should have built the test target
    runner.assert_ran("cmake", &["--build", "build", "--target", "testrun_tests"]);

    // Should have run ctest
    runner.assert_ran("ctest", &["--test-dir", "build"]);
}

#[test]
fn test_test_run_with_filter_and_jobs() {
    let (tmp, _) = setup_project_dir("testfilter");
    let runner = mock_with_conan_responses();

    // Create a test file so auto-detection finds it
    fs::create_dir_all(tmp.path().join("tests")).unwrap();
    fs::write(tmp.path().join("tests/test_math.cpp"), "// test\n").unwrap();

    let args = TestArgs {
        filter: Some("math".to_string()),
        jobs: Some(4),
        sanitizers: Sanitizers::default(),
        command: None,
    };

    with_cwd(tmp.path(), || {
        cmd_test(&runner, args, LevelFilter::Info).unwrap();
    });

    // ctest should have the filter and jobs args
    runner.assert_ran("ctest", &["--test-dir", "build", "-R", "math", "-j", "4"]);
}

#[test]
fn test_test_run_with_no_tests_prints_message() {
    let (tmp, _) = setup_project_dir("notests");
    let runner = mock_with_conan_responses();

    let args = TestArgs {
        filter: None,
        jobs: None,
        sanitizers: Sanitizers::default(),
        command: None,
    };

    // No test files exist => should return Ok without running cmake/ctest
    let result = with_cwd(tmp.path(), || cmd_test(&runner, args, LevelFilter::Info));
    assert!(result.is_ok());

    // Should NOT have run cmake or ctest
    runner.assert_did_not_run("cmake");
    runner.assert_did_not_run("ctest");
}

// ---------------------------------------------------------------------------
// cmd_clean tests
// ---------------------------------------------------------------------------

#[test]
fn test_clean_removes_cache_dir() {
    let (tmp, _) = setup_project_dir("cleantest");
    let runner = mock_with_conan_responses();

    // Verify build dir exists
    assert!(tmp.path().join("build").is_dir());

    with_cwd(tmp.path(), || {
        cmd_clean(&runner).unwrap();
    });

    // Build dir should be gone
    assert!(
        !tmp.path().join("build").is_dir(),
        "build directory should have been removed by cmd_clean"
    );
}

#[test]
fn test_clean_succeeds_when_no_cache_dir() {
    let (tmp, _) = setup_project_dir("cleannocache");
    let runner = mock_with_conan_responses();

    // Remove the build dir before calling clean
    fs::remove_dir_all(tmp.path().join("build")).unwrap();
    assert!(!tmp.path().join("build").is_dir());

    let result = with_cwd(tmp.path(), || cmd_clean(&runner));

    // Should succeed without error
    assert!(
        result.is_ok(),
        "cmd_clean should succeed even when cache dir doesn't exist"
    );
}

// ---------------------------------------------------------------------------
// lockfile tests
// ---------------------------------------------------------------------------

#[test]
fn test_build_calls_conan_lock_create() {
    let (tmp, _) = setup_project_dir("lockbuild");
    let runner = mock_with_conan_responses();

    with_cwd(tmp.path(), || {
        cmd_build(
            &runner,
            BuildType::Debug,
            LevelFilter::Info,
            &Sanitizers::default(),
        )
        .unwrap();
    });

    runner.assert_ran("conan", &["lock", "create"]);
}

#[test]
fn test_build_with_existing_lockfile_passes_it_to_install() {
    let (tmp, _) = setup_project_dir("lockexist");
    let runner = mock_with_conan_responses();

    // Create a lockfile before building
    fs::write(tmp.path().join("c3pg.lock"), "{}").unwrap();

    with_cwd(tmp.path(), || {
        cmd_build(
            &runner,
            BuildType::Debug,
            LevelFilter::Info,
            &Sanitizers::default(),
        )
        .unwrap();
    });

    // conan install should have received --lockfile=c3pg.lock
    runner.assert_ran("conan", &["install", "--lockfile=c3pg.lock"]);
}

#[test]
fn test_add_dependency_removes_lockfile() {
    let (tmp, _) = setup_project_dir("lockdel");
    let runner = mock_with_conan_responses();

    // Create a lockfile
    fs::write(tmp.path().join("c3pg.lock"), "{}").unwrap();
    assert!(tmp.path().join("c3pg.lock").exists());

    with_cwd(tmp.path(), || {
        cmd_add(&runner, "fmt").unwrap();
    });

    assert!(
        !tmp.path().join("c3pg.lock").exists(),
        "lockfile should be removed after cmd_add"
    );
}

#[test]
fn test_remove_dependency_removes_lockfile() {
    let (tmp, _) = setup_project_dir("lockrm");
    let runner = mock_with_conan_responses();

    // Add a dependency first
    with_cwd(tmp.path(), || {
        cmd_add(&runner, "fmt").unwrap();
    });

    // Create a lockfile
    fs::write(tmp.path().join("c3pg.lock"), "{}").unwrap();

    let runner2 = mock_with_conan_responses();
    with_cwd(tmp.path(), || {
        cmd_remove(&runner2, "fmt").unwrap();
    });

    assert!(
        !tmp.path().join("c3pg.lock").exists(),
        "lockfile should be removed after cmd_remove"
    );
}

// ---------------------------------------------------------------------------
// cmd_fmt / cmd_lint tests
// ---------------------------------------------------------------------------

#[test]
fn test_fmt_invokes_clang_format() {
    let (tmp, _) = setup_project_dir("fmttest");
    let runner = mock_with_conan_responses();

    with_cwd(tmp.path(), || {
        c3pg::format::cmd_fmt(&runner, LevelFilter::Info, "tests", false).unwrap();
    });

    runner.assert_ran("clang-format", &["-i"]);
    // Should include the main.cpp file
    let cmds = runner.executed_commands();
    let fmt_cmd = cmds.iter().find(|(c, _)| c == "clang-format").unwrap();
    assert!(
        fmt_cmd.1.iter().any(|a| a.contains("main.cpp")),
        "clang-format should receive main.cpp, got: {:?}",
        fmt_cmd.1
    );
}

#[test]
fn test_fmt_check_mode() {
    let (tmp, _) = setup_project_dir("fmtcheck");
    let runner = mock_with_conan_responses();

    with_cwd(tmp.path(), || {
        c3pg::format::cmd_fmt(&runner, LevelFilter::Info, "tests", true).unwrap();
    });

    runner.assert_ran("clang-format", &["--dry-run", "--Werror"]);
}

#[test]
fn test_lint_requires_compile_commands() {
    let (tmp, _) = setup_project_dir("lintnocc");
    let runner = mock_with_conan_responses();

    let result = with_cwd(tmp.path(), || {
        c3pg::format::cmd_lint(&runner, LevelFilter::Info, "tests", "build", false)
    });

    assert!(result.is_err());
    let err_msg = format!("{:#}", result.unwrap_err());
    assert!(
        err_msg.contains("compile_commands.json"),
        "Error should mention compile_commands.json, got: {err_msg}"
    );
}

#[test]
fn test_lint_invokes_clang_tidy() {
    let (tmp, _) = setup_project_dir("linttest");
    let runner = mock_with_conan_responses();

    // Create the compile_commands.json so the pre-check passes
    fs::write(tmp.path().join("build/compile_commands.json"), "[]").unwrap();

    with_cwd(tmp.path(), || {
        c3pg::format::cmd_lint(&runner, LevelFilter::Info, "tests", "build", false).unwrap();
    });

    runner.assert_ran("clang-tidy", &["-p=build"]);
}

#[test]
fn test_lint_fix_mode() {
    let (tmp, _) = setup_project_dir("lintfix");
    let runner = mock_with_conan_responses();

    fs::write(tmp.path().join("build/compile_commands.json"), "[]").unwrap();

    with_cwd(tmp.path(), || {
        c3pg::format::cmd_lint(&runner, LevelFilter::Info, "tests", "build", true).unwrap();
    });

    runner.assert_ran("clang-tidy", &["-p=build", "--fix"]);
}

// ---------------------------------------------------------------------------
// Multi-target tests
// ---------------------------------------------------------------------------

#[test]
fn test_multitarget_build_invokes_cmake() {
    let (tmp, _) = setup_multitarget_project("multitest");
    let runner = mock_with_conan_responses();

    with_cwd(tmp.path(), || {
        cmd_build(
            &runner,
            BuildType::Debug,
            LevelFilter::Info,
            &Sanitizers::default(),
        )
        .unwrap();
    });

    // Should have invoked conan install and cmake build
    runner.assert_ran("conan", &["install"]);
    runner.assert_ran("cmake", &["--build"]);
}

#[test]
fn test_multitarget_generates_correct_cmake() {
    let (tmp, _) = setup_multitarget_project("cmakecheck");
    let runner = mock_with_conan_responses();

    with_cwd(tmp.path(), || {
        // build_config regenerates CMakeLists.txt
        cmd_build(
            &runner,
            BuildType::Debug,
            LevelFilter::Info,
            &Sanitizers::default(),
        )
        .unwrap();
    });

    let cmake = fs::read_to_string(tmp.path().join("build/CMakeLists.txt")).unwrap();
    assert!(
        cmake.contains("add_library(mylib STATIC"),
        "Expected mylib library:\n{cmake}"
    );
    assert!(
        cmake.contains("add_executable(myapp"),
        "Expected myapp executable:\n{cmake}"
    );
    assert!(
        cmake.contains("add_executable(mytool"),
        "Expected mytool executable:\n{cmake}"
    );
}

#[test]
fn test_multitarget_run_requires_target() {
    let (tmp, _) = setup_multitarget_project("runmulti");
    let runner = mock_with_conan_responses();

    let result = with_cwd(tmp.path(), || {
        cmd_run(
            &runner,
            BuildType::Debug,
            LevelFilter::Info,
            &Sanitizers::default(),
            None,
        )
    });

    assert!(
        result.is_err(),
        "cmd_run without --target should fail with multiple executables"
    );
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("multiple") || err_msg.contains("--target"),
        "Error should mention multiple targets, got: {err_msg}"
    );
}

#[test]
fn test_multitarget_run_with_target() {
    let (tmp, _) = setup_multitarget_project("runtarget");
    let runner = mock_with_conan_responses();
    runner.on_success("build/myapp", &[], "Hello!");

    with_cwd(tmp.path(), || {
        cmd_run(
            &runner,
            BuildType::Debug,
            LevelFilter::Info,
            &Sanitizers::default(),
            Some("myapp"),
        )
        .unwrap();
    });

    runner.assert_ran("build/myapp", &[]);
}

#[test]
fn test_multitarget_run_with_wrong_target() {
    let (tmp, _) = setup_multitarget_project("runwrong");
    let runner = mock_with_conan_responses();

    let result = with_cwd(tmp.path(), || {
        cmd_run(
            &runner,
            BuildType::Debug,
            LevelFilter::Info,
            &Sanitizers::default(),
            Some("nonexistent"),
        )
    });

    assert!(result.is_err());
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("nonexistent"),
        "Error should mention the bad target name, got: {err_msg}"
    );
}

#[test]
fn test_multitarget_config_roundtrips() {
    // Write a config with targets, read it back, verify.
    let (tmp, original) = setup_multitarget_project("roundtrip");
    let readback = read_config(tmp.path());
    assert_eq!(readback.lib.len(), original.lib.len());
    assert_eq!(readback.bin.len(), original.bin.len());
    assert_eq!(readback.lib[0].name, "mylib");
    assert_eq!(readback.bin[0].name, "myapp");
    assert_eq!(readback.bin[1].name, "mytool");
}
