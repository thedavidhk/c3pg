# c3pg (C++ Playground)

*c3pg* (**C** **P**lus **P**lus **P**lay**G**round) is a command-line tool designed to simplify the process of creating,
managing, and running C++ project sandboxes. Inspired by Rust's `cargo`, it aims to make setting up
quick C++ test projects as easy and efficient as possible, even when working with external
dependencies.

While C++ development often involves managing complex build systems like CMake and dependency
managers like Conan, `c3pg` abstracts these details, allowing you to focus on writing and testing
code. The complexity still exists, but it stays under the hood.

## Features

- **Quick Project Setup**: Initialize a new C++ project sandbox with a single command.
- **Unified Configuration**: Use `c3pg.toml` for all project configuration, similar to `Cargo.toml` in Rust.
- **Dependency Management**: Easily add and remove Conan dependencies.
- **Build and Run**: Compile and execute your sandbox projects with minimal effort.
- **Testing**: Scaffold and run GTest-based tests with auto-detection.
- **Customizable C++ Standards**: Specify the C++ standard for your projects (e.g., C++20, C++17).
- **Git Integration**: Optionally initialize a Git repository for version control.

---

## Installation

`c3pg` requires the following tools to be installed on your system:

- [CMake](https://cmake.org/) (3.21+)
- [Conan (2.x)](https://conan.io/)
- A C++ compiler (GCC, Clang, or MSVC)

Install these tools via your package manager or their respective websites.

To build and install `c3pg`, use the Rust toolchain:

```bash
cargo install --path .
```

This will install the `c3pg` binary locally (by default in `$HOME/.cargo/bin`).

---

## Usage

### Overview

```bash
c3pg [COMMAND] [OPTIONS]
```

### Commands

#### `new`

Create a new C++ sandbox project.

```bash
c3pg new <sandbox_name> [OPTIONS]
```

Options:

- `--no-git`: Do not initialize a Git repository.
- `--standard`: Set the C++ standard for the project (default: C++20).

Example:

```bash
c3pg new my_sandbox --standard 17
```

#### `add`

Add a dependency to the current project.

```bash
c3pg add <dependency>
```

Example:

```bash
c3pg add fmt
```

`c3pg` looks for the latest version in the default Conan remote by default. Optionally, you can
specify a version and/or a user/channel:

```bash
c3pg add fmt/10.0.1
c3pg add fmt/10.0.1@some_user/some_channel
```

#### `remove`

Remove a dependency from the current project.

```bash
c3pg remove <dependency>
```

Example:

```bash
c3pg remove fmt
```

#### `build`

Build the current sandbox project.

```bash
c3pg build [OPTIONS]
```

Options:

- `--release, -r`: Build in release mode (default: debug).

Example:

```bash
c3pg build --release
```

#### `run`

Run the current sandbox project (builds first if necessary).

```bash
c3pg run [OPTIONS]
```

Options:

- `--release, -r`: Build in release mode (default: debug).

Example:

```bash
c3pg run
c3pg run --release
```

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

Examples:

```bash
# Create a test (lazily adds gtest on first use)
c3pg test add math

# Run all tests
c3pg test

# Run only tests matching "math" with 4 jobs
c3pg test --filter math --jobs 4
```

#### `clean`

Remove all build artifacts.

```bash
c3pg clean
```

---

## How It Works

### Project Structure

When you create a new sandbox, `c3pg` generates the following files:

```
my_project/
  c3pg.toml          # Unified project configuration
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

### Example Workflow

1. Create a new project:

```bash
c3pg new my_project
```

2. Add a dependency:

```bash
c3pg add fmt
```

3. Edit `src/main.cpp`:

```c++
#include <fmt/core.h>

int main() {
    fmt::print("Hello, world!\n");
}
```

4. Build and run:

```bash
c3pg run
```

5. Add and run tests:

```bash
c3pg test add math
# edit tests/test_math.cpp ...
c3pg test
```

---

## Configuration

### `c3pg.toml`

The `c3pg.toml` file is the central configuration file for your project. Here's an example:

```toml
[project]
name = "my_project"
dependencies = ["fmt/10.1.0", "gtest/1.15.0"]

[cmake]
standard = "Cpp20"
export_compile_commands = true

[conan]
bin = "conan"

[testing]
dir = "tests"
```

### Sections

- **`[project]`**: Project-level settings -- name, dependencies, and build cache directory.
- **`[cmake]`**: CMake-specific settings -- C++ standard and compile-commands export.
- **`[conan]`**: Conan-specific settings -- binary path and optional remote override.
- **`[testing]`**: Testing settings -- test source directory (default: `tests`).

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
