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
- **Unified Configuration**: Use c3pg.toml for all project configuration, similar to Cargo.toml in Rust.
- **Dependency Management**: Easily add Conan dependencies to your project.
- **Build and Run**: Compile and execute your sandbox projects with minimal effort.
- **Customizable C++ Standards**: Specify the C++ standard for your projects (e.g., C++20, C++17).
- **Git Integration**: Optionally initialize a Git repository for version control.

---

## Installation

`c3pg` requires the following tools to be installed on your system:

- [CMake](https://cmake.org/)
- [Conan (2.x)](https://conan.io/)

Install these tools via your package manager or their respective websites.

To build and run `c3pg`, use the Rust tool chain:

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

`c3pg` looks for the latest version in the default Conan remote by default. Optionally, we can
specify a version and/or a user/channel:

````bash
c3pg add fmt/10.0.1
c3pg add fmt/10.0.1@some_user/some_channel
```bash
`
````

#### `build`

Build the current sandbox project.

```bash
c3pg build [OPTIONS]
```

Options:

- `--build-type, -b`: Set the build type (`Debug`, `Release`, `RelWithDebugInfo`) (default:
  `Debug`).

Example:

```bash
c3pg build -b Release
```

#### `run`

Run the current sandbox project (builds first if necessary).

```bash
c3pg run [OPTIONS]
```

Options:

- `--build-type, -b`: Set the build type (`Debug`, `Release`, `RelWithDebugInfo`) (default:
  `Debug`).

Example:

```bash
c3pg run
```

---

## How It Works

### Project Structure

When you create a new sandbox, `c3pg` generates the following files:

- `c3pg.toml`: A unified configuration file for the project. This file includes all project
  settings, such as dependencies, the C++ standard, and Conan/CMake configurations.
- `main.cpp`: A simple "Hello World" program.
- `build/` directory: Contains all generated build files, including:
  - `CMakeLists.txt`: A minimal CMake configuration.
  - `conanfile.py`: A template Conan file for dependency management.
- `.gitignore`: A Git ignore file (if Git is initialized).

### Example Workflow

1. Create a new project:

```bash
c3pg new my_project
```

2. Add a dependency:

```bash
c3pg add fmt
```

3. Edit the generated `main.cpp` file, e.g.:

```c++
#include <fmt/core.h>

int main() {
    fmt::print("Hello, world!\n");
}
```

3. Build and run the project:

```bash
c3pg run
```

---

## Configuration

### `c3pg.toml`

The `c3pg.toml` file is the central configuration file for your project. Here's an example:

```toml
[project]
name = "my_project"
dependencies = ["fmt/10.1.0"]

[cmake]
standard = "20"
export_compile_commands = true

[conan]
bin = "conan"
remote = "default"
```

### Sections

- **`[project]`**: Project-level settings, such as the project name and dependencies.
- **`[cmake]`**: CMake-specific settings, such as the C++ standard.
- **`[conan]`**: Conan-specific settings, such as the Conan binary path and the default remote.

---

## Future Plans

While `c3pg` already simplifies sandbox creation and management, future iterations might include:

- Built-in templates for common project setups.
- Improved integration with package managers and remote repositories.
- Additional customization options for `c3pg.toml`.
