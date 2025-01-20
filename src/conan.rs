use anyhow::{anyhow, bail, Result};
use std::fmt;
use std::io::BufRead;
use std::process::Command;
use std::str::FromStr;

use crate::config::Config;
use crate::dependency::Dependency;
use crate::traits::ToFile;

#[derive(Debug, ToFile)]
pub struct Conan {
    bin: String,
    remote: String,
    requirements: Vec<Dependency>,
}

impl Conan {
    pub fn from_config(config: &Config) -> Result<Self> {
        Ok(Self {
            bin: config.conan.bin.clone(),
            remote: config
                .conan
                .remote
                .clone()
                .unwrap_or(Self::get_first_remote("conan")?),
            requirements: config.project.dependencies.clone(),
        })
    }

    pub fn install(&self, dir: &str, out_dir: &str) -> Result<()> {
        let conan_status = Command::new(&self.bin)
            .args([
                "install",
                "--build=missing",
                "--output-folder",
                out_dir,
                dir,
            ])
            .status()?;
        if !conan_status.success() {
            bail!("Conan install failed");
        }
        Ok(())
    }

    pub fn get_latest_matching_dependency(&self, expr: &str) -> Option<Dependency> {
        let Ok(dependency) = Dependency::from_str(expr) else {
            println!("Could not parse {} into a dependency.", expr);
            return None;
        };
        self.find_dependency(dependency)
    }

    fn find_dependency(&self, dependency: Dependency) -> Option<Dependency> {
        let search_result = self.search(dependency.name.as_str()).ok()?;
        let matching_versions = search_result
            .lines()
            .filter_map(|line| Dependency::from_str(line).ok())
            .filter(|entry| entry.matches(&dependency));
        matching_versions.last()
    }

    fn search(&self, expr: &str) -> Result<String> {
        let output = Command::new(&self.bin)
            .args(["search", "-r", &self.remote, expr])
            .output()?;
        if !output.status.success() {
            bail!("Could not search in remote {}", &self.remote);
        }

        Ok(String::from_utf8_lossy(&output.stdout).into())
    }

    fn get_first_remote(bin: &str) -> Result<String> {
        let remote = Command::new(bin)
            .args(["remote", "list"])
            .output()?
            .stdout
            .lines()
            .next()
            .ok_or(anyhow!("remotes list is empty"))??;
        let remote_name = remote
            .split(":")
            .next()
            .ok_or(anyhow!("remote not found"))?;
        Ok(String::from(remote_name))
    }
}

impl fmt::Display for Conan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let requirements = if self.requirements.is_empty() {
            "        pass".to_string()
        } else {
            self.requirements
                .iter()
                .map(|dep| format!("        self.requires(\"{}\")", dep))
                .collect::<Vec<_>>()
                .join("\n")
        };

        write!(
            f,
            r#"
from conan import ConanFile

class SandboxConan(ConanFile):
    name = "sandbox"
    version = "0.1"
    settings = "os", "compiler", "build_type", "arch"
    generators = "CMakeDeps", "CMakeToolchain"

    def requirements(self):
{requirements}
"#,
            requirements = requirements
        )
    }
}
