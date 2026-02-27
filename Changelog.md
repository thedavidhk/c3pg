# Changelog

## 0.3.1 - 2026-02-27

### Added
- `c3pg scratch` command for quick throwaway C++ projects in a temporary directory.
- `--print-path` flag on `new` and `scratch` for shell composition (`cd $(c3pg new myapp --print-path)`).

## 0.3.0 - 2026-02-15

### Added
- `c3pg init` command to initialize a project in an existing directory.
- `c3pg fmt` and `c3pg lint` commands wrapping `clang-format` and `clang-tidy`.
- Sanitizer profiles: `--asan`, `--tsan`, `--ubsan` flags for `build`, `run`, and `test`.
- Lockfile support (`c3pg.lock`) for reproducible Conan dependency resolution.
- Multi-target support via `[[lib]]` and `[[bin]]` sections in `c3pg.toml`.
- Convention-over-configuration: simple projects need no target declarations.
- Cargo-style `[dependencies]` table with simple (`"version"`) and detailed
  (`{ version, user, channel }`) inline-table forms.
- Dependency cycle detection and validation for target link graphs.
- Preflight diagnostics with actionable hints for common build errors.
- Integration and end-to-end test suites.
- GitHub Actions CI/CD workflows for testing and crates.io publishing.
- MIT license.

### Changed
- C++ standard is now a first-class `[project]` field (moved from `[cmake]`).
- Logging and output overhaul: colored compiler output is preserved, duplicate
  messages eliminated, error context is propagated without swallowing root causes.
- `c3pg test` now aborts immediately if the build step fails.
- Default-valued optional sections (`[cmake]`, `[conan]`, `[testing]`) are omitted
  from `c3pg.toml` for a cleaner config.
- Internal CMake generation rewritten using a typed AST (`cmake_core`).
- Proc-macro crate renamed from `macros` to `c3pg-macros` for crates.io compatibility.

### Fixed
- Conan toolchain file path resolution (`build/conan_toolchain.cmake` not found).
- Header include paths for dependencies used outside of `main.cpp`.
- Removed invalid `-stdlib=libstdc++` flag on GCC.

## 0.2.0 - 2025-08-25

### Changed
- Rename project (including config file) from "cpppg" to "c3pg".
- Include all source files in `src/` in CMake (not just `main.cpp`).

### Fixed
- Mismatched build type between Conan and CMake.
- Incorrect target name in generated `CMakeLists.txt`.

## 0.1.0 - 2025-01-30

### Added
- `new` command to scaffold a C++ project with CMake and Conan.
- `add` command to search and add Conan dependencies.
- `remove` command to remove dependencies.
- `build` command to compile via Conan install + CMake.
- `run` command to build and execute the project binary.
- `clean` command to remove build artifacts.
- `cpppg.toml` configuration file for project settings.
- Generated `CMakeLists.txt` and `conanfile.py` kept in the build directory.
- Duplicate dependency prevention.
- `CommandRunner` abstraction for testable process execution.
- Proc-macro-derived `FromFile`/`ToFile` traits for config serialization.
