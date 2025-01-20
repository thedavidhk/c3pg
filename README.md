# cpppg (C++ Playground)

**cpppg (C++ Playground)** is a command-line tool designed to simplify the process of creating,
managing, and running C++ project sandboxes. Inspired by Rust's `cargo`, it aims to make setting up
quick C++ test projects as easy and efficient as possible, even when working with external
dependencies.

While C++ development often involves managing complex build systems like CMake and dependency
managers like Conan, `cpppg` abstracts these details, allowing you to focus on writing and testing
code. The complexity still exists, but it stays under the hood.

## Features

- **Quick Project Setup**: Initialize a new C++ project sandbox with a single command.
- **Dependency Management**: Easily add Conan dependencies to your project.
- **Build and Run**: Compile and execute your sandbox projects with minimal effort.
- **Customizable C++ Standards**: Specify the C++ standard for your projects (e.g., C++20, C++17).
- **Git Integration**: Optionally initialize a Git repository for version control.

## Installation

`cpppg` requires the following tools to be installed on your system:

- [CMake](https://cmake.org/)
- [Conan (2.x)](https://conan.io/)

Install these tools via your package manager or their respective websites.

To build and run `cpppg`, use the Rust tool chain:

`cargo install --path .`

This will install the `cpppg` binary locally (by default in `$HOME/.cargo/bin`).

## Usage

### Overview

`cpppg [COMMAND] [OPTIONS]`

### Commands

#### `new`

Create a new C++ sandbox project.

`cpppg new <sandbox_name> [OPTIONS]`

Options:

- `--no-git`: Do not initialize a Git repository.
- `--standard`: Set the C++ standard for the project (default: C++20).

Example:

`cpppg new my_sandbox --standard 17`

#### `add`

Add a dependency to the current project.

`cpppg add <dependency>`

Example:

`cpppg add fmt`

`cpppg` looks for the latest version in the default Conan remote by default. Optionally, we can
specify a version and/or a user/channel:

`cpppg add fmt/10.0.1`

or

`cpppg add fmt/10.0.1@some_user/some_channel`

#### `build`

Build the current sandbox project.

`cpppg build [OPTIONS]`

Options:

- `--build-type, -b`: Set the build type (`Debug`, `Release`, `RelWithDebugInfo`) (default:
  `Debug`).

Example:

`cpppg build -b Release`

#### `run`

Run the current sandbox project (builds first if necessary).

`cpppg run [OPTIONS]`

Options:

- `--build-type, -b`: Set the build type (`Debug`, `Release`, `RelWithDebugInfo`) (default:
  `Debug`).

Example:

`cpppg run`

## How It Works

### Project Structure

When you create a new sandbox, `cpppg` generates the following files:

- `main.cpp`: A simple "Hello World" program.
- `CMakeLists.txt`: A minimal CMake configuration.
- `conanfile.py`: A template Conan file for dependency management.
- `.gitignore`: A Git ignore file (if Git is initialized).

### Conan and CMake

`cpppg` relies on Conan for managing dependencies and CMake for building the project. These tools
must be installed on your system. Future versions might aim to streamline this further by
introducing a unified configuration file (similar to Rust's `Cargo.toml`).

### Example Workflow

1. Create a new project:

   `cpppg new my_project`

2. Add a dependency:

   `cpppg add fmt`

3. Edit the generated `main.cpp` file, e.g.:

```c++
#include <fmt/core.h>

int main() {
    fmt::print("Hello, world!\n");
}
```

3. Build and run the project:

   `cpppg run`

## Future Plans

While `cpppg` already simplifies sandbox creation and management, future iterations might include:

- A unified configuration file (e.g., `sandbox.toml`) to replace Conan and CMake files.
- Built-in templates for common project setups.
- Improved integration with package managers and remote repositories.
