use crate::command_runner::CommandRunner;
use crate::ui;

/// Verify that a tool is reachable on `$PATH` by running `<tool> --version`.
///
/// Returns `Ok(())` if the tool is found, or an error describing what is
/// missing and how to fix it.
fn check_tool(runner: &impl CommandRunner, name: &str, install_hint: &str) -> anyhow::Result<()> {
    let result = runner.command(name).args(["--version"]).run();
    match result {
        Ok(r) if r.success => Ok(()),
        _ => anyhow::bail!(
            "`{name}` not found on PATH -- {install_hint}"
        ),
    }
}

/// Run lightweight pre-flight checks before a build.
///
/// Verifies that `cmake`, `conan`, and a C++ compiler are reachable.
/// This catches the most common "tool not installed" errors early and
/// produces a clear, actionable message instead of a cryptic failure deep
/// inside cmake or conan.
pub fn preflight_build(runner: &impl CommandRunner) {
    if let Err(e) = check_tool(runner, "cmake", "install CMake (https://cmake.org)") {
        ui::warn(&format!("{e}"));
    }
    if let Err(e) = check_tool(runner, "conan", "install Conan 2 (`pip install conan`)") {
        ui::warn(&format!("{e}"));
    }
    // Check for a C++ compiler: prefer CXX env var, fall back to c++ / g++.
    let cxx = std::env::var("CXX").unwrap_or_default();
    let compilers: &[&str] = if cxx.is_empty() {
        &["c++", "g++", "clang++"]
    } else {
        // Leak is fine: this runs at most once per process.
        // We avoid allocation by checking the env var directly.
        return; // CXX is set — trust the user.
    };
    let found = compilers
        .iter()
        .any(|cc| check_tool(runner, cc, "").is_ok());
    if !found {
        ui::warn("no C++ compiler found on PATH -- install g++ or clang++");
    }
}

/// Match an error message against known failure patterns and return a
/// human-friendly hint, or `None` if no pattern matches.
#[must_use]
pub fn hint_for_error(msg: &str) -> Option<&'static str> {
    static PATTERNS: &[(&str, &str)] = &[
        (
            "Could not find toolchain file",
            "hint: run `c3pg build` first to generate the Conan toolchain",
        ),
        (
            "CMAKE_MAKE_PROGRAM is not set",
            "hint: install a build tool such as `make` or `ninja`",
        ),
        (
            "unrecognized command-line option '-stdlib",
            "hint: your compiler does not support -stdlib; check your Conan profile \
             (`conan profile detect --force`)",
        ),
        (
            "file not found",
            "hint: ensure the dependency providing this header is listed in c3pg.toml \
             (`c3pg add <pkg>`)",
        ),
        (
            "No module named 'conans",
            "hint: stale Conan 1 plugins detected; remove custom commands from \
             ~/.conan2/extensions/commands/ or reinstall Conan 2",
        ),
        (
            "Failed to load c3pg.toml",
            "hint: are you inside a c3pg project? Create one with `c3pg new` or `c3pg init`",
        ),
    ];

    for &(pattern, hint) in PATTERNS {
        if msg.contains(pattern) {
            return Some(hint);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::MockCommandRunner;

    #[test]
    fn test_check_tool_found() {
        let runner = MockCommandRunner::default();
        assert!(check_tool(&runner, "cmake", "install cmake").is_ok());
    }

    #[test]
    fn test_check_tool_not_found() {
        let runner = MockCommandRunner::new(None); // default = failure
        let result = check_tool(&runner, "cmake", "install cmake");
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("cmake"));
        assert!(msg.contains("install cmake"));
    }

    #[test]
    fn test_hint_for_error_matches() {
        assert!(hint_for_error("Could not find toolchain file: build/conan_toolchain.cmake")
            .unwrap()
            .contains("c3pg build"));
    }

    #[test]
    fn test_hint_for_error_no_match() {
        assert!(hint_for_error("some random error").is_none());
    }

    #[test]
    fn test_hint_for_error_cmake_make_program() {
        let hint = hint_for_error("CMAKE_MAKE_PROGRAM is not set").unwrap();
        assert!(hint.contains("make") || hint.contains("ninja"));
    }

    #[test]
    fn test_hint_for_error_config_not_found() {
        let hint = hint_for_error("Failed to load c3pg.toml or cpppg.toml").unwrap();
        assert!(hint.contains("c3pg new") || hint.contains("c3pg init"));
    }
}
