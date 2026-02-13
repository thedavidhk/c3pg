//! End-to-end tests that exercise the real `c3pg` binary against real
//! cmake / conan / C++ toolchain.
//!
//! These tests are **disabled by default** because they require external tools.
//! Enable them by setting the environment variable:
//!
//! ```sh
//! C3PG_E2E=1 cargo test --test e2e
//! ```

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Return true when the e2e suite should actually run.
fn enabled() -> bool {
    std::env::var("C3PG_E2E").is_ok()
}

/// Shortcut: build a `Command` for the `c3pg` cargo binary.
#[allow(deprecated)] // cargo_bin still works; the macro replacement is unstable
fn c3pg() -> Command {
    Command::cargo_bin("c3pg").expect("c3pg binary not found")
}

// ---------------------------------------------------------------------------
// Smoke test: new → build → run
// ---------------------------------------------------------------------------

#[test]
fn e2e_new_build_run() {
    if !enabled() {
        eprintln!("skipping e2e (set C3PG_E2E=1 to enable)");
        return;
    }

    let tmp = TempDir::new().unwrap();

    // --- c3pg new -----------------------------------------------------------
    c3pg()
        .args(["new", "hello", "--no-git"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let project = tmp.path().join("hello");
    assert!(project.join("src/main.cpp").exists());
    assert!(project.join("c3pg.toml").exists());
    assert!(project.join("build/CMakeLists.txt").exists());
    assert!(project.join("build/conanfile.py").exists());

    // New projects should not contain gtest (lazy setup via `test add`)
    let config = fs::read_to_string(project.join("c3pg.toml")).unwrap();
    assert!(
        !config.contains("gtest"),
        "Fresh project should not have gtest dependency"
    );

    // --- c3pg build ---------------------------------------------------------
    c3pg()
        .args(["build"])
        .current_dir(&project)
        .assert()
        .success();

    // The binary should exist after build
    let binary = project.join("build/hello");
    assert!(
        binary.exists(),
        "Expected binary at {}, directory contents: {:?}",
        binary.display(),
        fs::read_dir(project.join("build"))
            .map(|rd| rd
                .filter_map(std::result::Result::ok)
                .map(|e| e.file_name())
                .collect::<Vec<_>>())
            .unwrap_or_default(),
    );

    // --- c3pg run -----------------------------------------------------------
    c3pg()
        .args(["run", "-v"])
        .current_dir(&project)
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello from C3PG!"));

    // --- c3pg build --release -----------------------------------------------
    c3pg()
        .args(["build", "--release"])
        .current_dir(&project)
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Dependency management: add → build → remove → build
// ---------------------------------------------------------------------------

#[test]
fn e2e_add_remove_dependency() {
    if !enabled() {
        eprintln!("skipping e2e (set C3PG_E2E=1 to enable)");
        return;
    }

    let tmp = TempDir::new().unwrap();

    // Create project
    c3pg()
        .args(["new", "deptest", "--no-git"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let project = tmp.path().join("deptest");

    // Add fmt
    c3pg()
        .args(["add", "fmt"])
        .current_dir(&project)
        .assert()
        .success();

    // Verify fmt appears in config
    let config = fs::read_to_string(project.join("c3pg.toml")).unwrap();
    assert!(
        config.contains("fmt"),
        "Expected c3pg.toml to mention fmt after add, got:\n{config}"
    );

    // Build should still succeed with the new dependency
    c3pg()
        .args(["build"])
        .current_dir(&project)
        .assert()
        .success();

    // Remove fmt
    c3pg()
        .args(["remove", "fmt"])
        .current_dir(&project)
        .assert()
        .success();

    // Config should no longer mention fmt
    let config_after = fs::read_to_string(project.join("c3pg.toml")).unwrap();
    assert!(
        !config_after.contains("fmt"),
        "Expected c3pg.toml to NOT mention fmt after remove, got:\n{config_after}"
    );
}

// ---------------------------------------------------------------------------
// Test scaffold: test add → test (run)
// ---------------------------------------------------------------------------

#[test]
fn e2e_test_add_and_run() {
    if !enabled() {
        eprintln!("skipping e2e (set C3PG_E2E=1 to enable)");
        return;
    }

    let tmp = TempDir::new().unwrap();

    // Create project
    c3pg()
        .args(["new", "testproj", "--no-git"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let project = tmp.path().join("testproj");

    // Running `test` without any test files should not fail
    c3pg()
        .args(["test"])
        .current_dir(&project)
        .assert()
        .success();

    // Add a test (this lazily adds gtest)
    c3pg()
        .args(["test", "add", "math"])
        .current_dir(&project)
        .assert()
        .success();

    assert!(project.join("tests/test_math.cpp").exists());

    // Config should now contain gtest
    let config = fs::read_to_string(project.join("c3pg.toml")).unwrap();
    assert!(
        config.contains("gtest"),
        "Expected gtest in config after `test add`, got:\n{config}"
    );

    // Run tests (build + ctest)
    c3pg()
        .args(["test"])
        .current_dir(&project)
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Clean
// ---------------------------------------------------------------------------

#[test]
fn e2e_clean() {
    if !enabled() {
        eprintln!("skipping e2e (set C3PG_E2E=1 to enable)");
        return;
    }

    let tmp = TempDir::new().unwrap();

    c3pg()
        .args(["new", "cleanme", "--no-git"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let project = tmp.path().join("cleanme");

    // Build first so there are artifacts
    c3pg()
        .args(["build"])
        .current_dir(&project)
        .assert()
        .success();

    assert!(project.join("build").is_dir());

    // Clean
    c3pg()
        .args(["clean"])
        .current_dir(&project)
        .assert()
        .success();

    assert!(
        !project.join("build").is_dir(),
        "build/ should be removed after clean"
    );
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn e2e_build_outside_project_fails() {
    if !enabled() {
        eprintln!("skipping e2e (set C3PG_E2E=1 to enable)");
        return;
    }

    let tmp = TempDir::new().unwrap();

    // Running build in a directory without c3pg.toml should fail
    c3pg()
        .args(["build"])
        .current_dir(tmp.path())
        .assert()
        .failure();
}

#[test]
fn e2e_remove_nonexistent_fails() {
    if !enabled() {
        eprintln!("skipping e2e (set C3PG_E2E=1 to enable)");
        return;
    }

    let tmp = TempDir::new().unwrap();

    c3pg()
        .args(["new", "rmfail", "--no-git"])
        .current_dir(tmp.path())
        .assert()
        .success();

    let project = tmp.path().join("rmfail");

    c3pg()
        .args(["remove", "doesnotexist"])
        .current_dir(&project)
        .assert()
        .failure();
}
