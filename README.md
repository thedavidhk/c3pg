# c3pg (C++ Playground)

*c3pg* (**C** **P**lus **P**lus **P**lay**G**round) is a command-line tool designed to simplify the process of creating,
managing, and running C++ projects. Inspired by Rust's `cargo`, it aims to make setting up
C++ projects as easy and efficient as possible, even when working with external dependencies.

While C++ development often involves managing complex build systems like CMake and dependency
managers like Conan, `c3pg` abstracts these details, allowing you to focus on writing and testing
code. The complexity still exists, but it stays under the hood.

## Features

- **Quick Project Setup**: Initialize a new C++ project with a single command.
- **Convention over Configuration**: Simple projects need only a project name -- no build targets
  to declare.
- **Unified Configuration**: Use `c3pg.toml` for all project configuration, similar to `Cargo.toml`
  in Rust.
- **Dependency Management**: Easily add and remove Conan dependencies with a Cargo-like
  `[dependencies]` table.
- **Build and Run**: Compile and execute your projects with minimal effort.
- **Testing**: Scaffold and run GTest-based tests with auto-detection.
- **Multi-target Support**: Declare `[[lib]]` and `[[bin]]` targets explicitly when needed.
- **Code Quality**: Format and lint C++ sources with `clang-format` and `clang-tidy`.
- **Sanitizers**: Enable AddressSanitizer, ThreadSanitizer, or UndefinedBehaviorSanitizer with a
  single flag.
- **Customizable C++ Standards**: Specify the C++ standard for your projects (default: C++20).
- **Git Integration**: Optionally initialize a Git repository for version control.

---

## Installation

`c3pg` requires the following tools to be installed on your system:

- [CMake](https://cmake.org/) (3.21+)
- [Conan (2.x)](https://conan.io/)
- A C++ compiler (GCC, Clang, or MSVC)

Install these tools via your package manager or their respective websites.

To build and install `c3pg` you need the Rust toolchain (see e.g.
[rustup.rs](https://rustup.rs/)).

To pull the latest release from [crates.io](crates.io) use

```bash
cargo install c3pg
```

or to build it locally from this git repo, clone it and run

```bash
cargo install --path .
```

This will install the `c3pg` binary locally (by default in `$HOME/.cargo/bin`).

---

## Quick Start

```bash
# Create a new project
c3pg new hello

# Add a dependency, build, and run
cd hello
c3pg add fmt
# edit src/main.cpp to use fmt ...
c3pg run

# Add and run tests
c3pg test add math
# edit tests/test_math.cpp ...
c3pg test
```

---

## Usage

### Commands

#### `new`

Create a new C++ project.

```bash
c3pg new <name> [OPTIONS]
```

Options:

- `--no-git`: Do not initialize a Git repository.
- `--standard`: Set the C++ standard (default: C++20).

Example:

```bash
c3pg new my_project --standard 17
```

#### `init`

Initialize a `c3pg` project in the current directory.

```bash
c3pg init [OPTIONS]
```

Options:

- `--no-git`: Do not initialize a Git repository.
- `--standard`: Set the C++ standard (default: C++20).

If `src/` already contains source files, they are left untouched. Otherwise a
starter `main.cpp` is created.

#### `add`

Add a dependency to the current project.

```bash
c3pg add <dependency>
```

Example:

```bash
c3pg add fmt
```

`c3pg` looks for the latest version in the default Conan remote. Optionally, you can
specify a version and/or a user/channel:

```bash
c3pg add fmt/10.0.1
c3pg add mylib/1.0.0@myuser/stable
```

#### `remove`

Remove a dependency from the current project.

```bash
c3pg remove <dependency>
```

#### `build`

Build the current project.

```bash
c3pg build [OPTIONS]
```

Options:

- `--release, -r`: Build in release mode (default: debug).
- `--asan`: Enable AddressSanitizer.
- `--tsan`: Enable ThreadSanitizer.
- `--ubsan`: Enable UndefinedBehaviorSanitizer.

#### `run`

Run the current project (builds first if necessary).

```bash
c3pg run [OPTIONS]
```

Options:

- `--release, -r`: Build in release mode (default: debug).
- `--target <name>`: Which executable to run (required when multiple exist).
- `--asan`, `--tsan`, `--ubsan`: Enable sanitizers.

#### `test`

Manage and run the project's test suite. GTest is added automatically the first time a test is
created.

```bash
c3pg test [OPTIONS]
c3pg test add <name>
```

Subcommands:

- `add <name>`: Scaffold a new GTest source file (`tests/test_<name>.cpp`). On first use, this
  also adds `gtest` as a dependency.

Options (when running tests):

- `--filter, -f`: Expression to match test cases to run.
- `--jobs, -j`: Number of parallel test jobs.
- `--asan`, `--tsan`, `--ubsan`: Enable sanitizers.

Examples:

```bash
# Create a test (lazily adds gtest on first use)
c3pg test add math

# Run all tests
c3pg test

# Run only tests matching "math" with 4 jobs
c3pg test --filter math --jobs 4
```

#### `fmt`

Format C/C++ source files with `clang-format`.

```bash
c3pg fmt [OPTIONS]
```

Options:

- `--check`: Check formatting without modifying files (exit with error if unformatted).

#### `lint`

Lint C/C++ source files with `clang-tidy` (requires a prior `c3pg build` for
`compile_commands.json`).

```bash
c3pg lint [OPTIONS]
```

Options:

- `--fix`: Apply suggested fixes in-place.

#### `clean`

Remove all build artifacts.

```bash
c3pg clean
```

---

## How It Works

### Project Structure

When you create a new project, `c3pg` generates the following:

```
my_project/
  c3pg.toml          # Project configuration
  src/
    main.cpp         # "Hello World" starter
  build/
    CMakeLists.txt   # Generated CMake configuration
    conanfile.py     # Generated Conan recipe
  .gitignore         # (if Git is initialized)
```

Tests are added on demand via `c3pg test add`:

```
my_project/
  tests/
    test_math.cpp    # Scaffolded GTest file
```

---

## Configuration

### `c3pg.toml`

The configuration follows a convention-over-configuration approach. A fresh project
needs only a project name:

```toml
[project]
name = "my_project"
standard = "Cpp20"
```

All sources in `src/` are compiled into a single executable named after the project.
This is the default when no `[[lib]]`/`[[bin]]` sections are present.

#### Dependencies

Dependencies are declared in a `[dependencies]` table, similar to `Cargo.toml`. A
simple dependency only needs a version string. For packages from a private Conan
remote you can specify `user` and `channel` using inline-table syntax:

```toml
[project]
name = "my_project"
standard = "Cpp20"

[dependencies]
fmt = "11.0.0"
boost = "1.88"
mylib = { version = "2.1.0", user = "team", channel = "stable" }
```

#### Multi-target project (library + executables)

When you need library targets or multiple executables, add `[[lib]]` and `[[bin]]`
sections (similar to Cargo's `[[bin]]` / `[lib]`):

```toml
[project]
name = "my_project"

[dependencies]
fmt = "11.0.0"

[[lib]]
name = "mylib"
sources = ["src/lib"]
public-include = ["include"]

[[bin]]
name = "my_project"
sources = ["src/main.cpp"]
link = ["mylib"]
```

#### Optional sections

These sections can be added to override defaults:

```toml
[cmake]
export_compile_commands = false   # default: true

[conan]
bin = "/usr/local/bin/conan"      # default: "conan"
remote = "my_remote"              # default: first configured remote

[testing]
dir = "test"                      # default: "tests"
```

### Sections

- **`[project]`**: Project name, C++ standard, and build cache directory.
- **`[dependencies]`** (optional): Conan packages with version strings or detailed
  inline tables (`{ version, user, channel }`).
- **`[[lib]]`** (optional): Library targets with sources, public include dirs, and
  inter-library linking.
- **`[[bin]]`** (optional): Executable targets with sources and library linking.
  Omit both `[[lib]]` and `[[bin]]` for convention-based single-executable projects.
- **`[cmake]`** (optional): CMake-specific settings.
- **`[conan]`** (optional): Conan binary path and remote override.
- **`[testing]`** (optional): Test source directory.

---

## Development

### Running tests

```bash
# Unit + integration tests (no external tools needed)
cargo test

# End-to-end tests (requires cmake, conan, and a C++ compiler)
C3PG_E2E=1 cargo test --test e2e
```

### Linting

```bash
cargo clippy --all-targets
```
