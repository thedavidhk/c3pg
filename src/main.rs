use clap::{Parser, Subcommand};
use std::error::Error;

/// Top-level CLI parser.
#[derive(Parser, Debug)]
#[command(name = "cpp_sandbox")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// List of subcommands.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a new sandbox directory with the given name
    New {
        /// The name of the new sandbox directory
        sandbox_name: String,
    },
    /// Add a Conan dependency to the current sandbox (in the current working directory)
    Add {
        /// Name of the Conan dependency (e.g. fmt/10.1.0)
        dependency: String,
    },
    /// Build the current sandbox project (in the current working directory)
    Build,
    /// Run the current sandbox project (build if necessary)
    Run,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { sandbox_name } => cmd_new(&sandbox_name)?,
        Commands::Add { dependency } => cmd_add(&dependency)?,
        Commands::Build => cmd_build()?,
        Commands::Run => cmd_run()?,
    }
    Ok(())
}

/// Create a new sandbox directory with a minimal setup (CMakeLists.txt, conanfile.py, main.cpp).
fn cmd_new(sandbox_name: &str) -> Result<(), Box<dyn Error>> {
    // 1. Create the sandbox directory
    std::fs::create_dir(sandbox_name)?;

    // 2. Write main.cpp
    let main_cpp_content = r#"#include <iostream>

int main() {
    std::cout << "Hello from C++ sandbox!" << std::endl;
    return 0;
}
"#;
    std::fs::write(format!("{}/main.cpp", sandbox_name), main_cpp_content)?;

    // 3. Write a minimal CMakeLists.txt
    let cmake_lists_content = format!(
        r#"cmake_minimum_required(VERSION 3.15)
project({} LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Include Conan-generated cmake files
# Typically: include(${{CMAKE_BINARY_DIR}}/conan_deps.cmake)

add_executable(sandbox main.cpp)
"#,
        sandbox_name
    );

    std::fs::write(
        format!("{}/CMakeLists.txt", sandbox_name),
        cmake_lists_content,
    )?;

    // 4. Write a minimal conanfile.py
    let conanfile_content = r#"from conan import ConanFile

class SandboxConan(ConanFile):
    name = "sandbox"
    version = "0.1"
    settings = "os", "compiler", "build_type", "arch"
    generators = "CMakeDeps", "CMakeToolchain"

    def requirements(self):
        pass  # Add dependencies here using self.requires(...)
"#;
    std::fs::write(format!("{}/conanfile.py", sandbox_name), conanfile_content)?;

    // 5. (Optional) write a .gitignore
    let gitignore_content = r#"build/
"#;
    std::fs::write(format!("{}/.gitignore", sandbox_name), gitignore_content)?;

    println!("Created new sandbox: {}", sandbox_name);
    Ok(())
}

/// Add a Conan dependency to conanfile.py in the current directory.
fn cmd_add(dependency: &str) -> Result<(), Box<dyn Error>> {
    // 1. Read existing conanfile.py
    let conanfile_path = "conanfile.py";
    let contents = std::fs::read_to_string(conanfile_path)?;

    // 2. Insert the dependency into the `requirements()` function
    // Very naive approach: find the line containing `def requirements(self):`
    // and insert a `self.requires("<dependency>")` after that line
    let mut new_contents = String::new();
    let mut inserted = false;
    for line in contents.lines() {
        new_contents.push_str(line);
        new_contents.push('\n');

        if line.trim_start().starts_with("def requirements(self):") {
            // Insert the new dependency line after this
            new_contents.push_str(&format!("        self.requires(\"{}\")\n", dependency));
            inserted = true;
        }
    }

    if !inserted {
        eprintln!("Warning: Could not find `def requirements(self):` in conanfile.py");
        // You might choose to append it at the end or handle differently
    }

    // 3. Write back the file
    std::fs::write(conanfile_path, new_contents)?;

    println!("Added dependency '{}' to conanfile.py", dependency);
    Ok(())
}

/// Build the current sandbox project.
/// Steps:
///   1. `conan install . --build=missing --output-folder=build`
///   2. `cmake -B build -DCMAKE_TOOLCHAIN_FILE=build/conan_toolchain.cmake -DCMAKE_BUILD_TYPE=Release`
///   3. `cmake --build build`
fn cmd_build() -> Result<(), Box<dyn Error>> {
    use std::process::Command;

    // Step 1: conan install
    let conan_status = Command::new("conan")
        .args(["install", ".", "--build=missing", "--output-folder=build"])
        .status()?;
    if !conan_status.success() {
        return Err("Conan install failed".into());
    }

    // Step 2: cmake configure
    let cmake_configure = Command::new("cmake")
        .args([
            "-B",
            "build",
            "-DCMAKE_TOOLCHAIN_FILE=build/conan_toolchain.cmake",
            "-DCMAKE_BUILD_TYPE=Release",
            "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON",
        ])
        .status()?;
    if !cmake_configure.success() {
        return Err("CMake configure failed".into());
    }

    // Step 3: cmake --build
    let cmake_build = Command::new("cmake").args(["--build", "build"]).status()?;
    if !cmake_build.success() {
        return Err("CMake build failed".into());
    }

    println!("Build successful!");
    Ok(())
}

/// Run the current sandbox project.
/// If the binary does not exist or is out of date, rebuild first, then run.
fn cmd_run() -> Result<(), Box<dyn Error>> {
    // 1. You might do a quick check if build binary is up to date, or just call `cmd_build()`:
    cmd_build()?;

    // 2. Run the resulting binary:
    //    For simplicity, assume the binary name is the same as the directory name,
    //    or maybe just a generic "sandbox" name. In the code above, we actually used
    //    the project name as the "executable" name. Let's guess the user’s directory name.
    //    You could parse `project(...)` from CMakeLists.txt or just guess "sandbox".
    //    For now, let's assume an output name "sandbox".
    //    (If your code has a better naming scheme, adjust accordingly.)

    let binary_path = "./build/sandbox"; // or "./build/my_sandbox"
    if cfg!(target_os = "windows") {
        // On Windows, it would be something like "build\sandbox.exe"
        std::process::Command::new(format!("{}.exe", binary_path)).status()?;
    } else {
        std::process::Command::new(binary_path).status()?;
    }

    Ok(())
}
