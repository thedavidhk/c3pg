use anyhow::Context;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt, str::FromStr};

use crate::{
    cmake::CppStandard,
    dependency::Dependency,
    traits::{FromFile, ToFile},
};

#[derive(Debug, Default, FromFile, ToFile, Serialize, Deserialize)]
pub struct Config {
    pub project: Project,
    pub cmake: CMakeConfig,
    pub conan: ConanConfig,
    pub testing: TestingConfig,
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let contents = toml::to_string_pretty(self).expect("Could not serialize Config to toml");
        write!(f, "{contents}")
    }
}

impl FromStr for Config {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        toml::from_str(s).context("Failed to parse the config string as TOML")
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    #[serde(default)]
    pub name: String,

    #[serde(
        serialize_with = "serialize_dependencies",
        deserialize_with = "deserialize_dependencies"
    )]
    pub dependencies: Vec<Dependency>,

    #[serde(
        default = "Project::default_cache_dir",
        skip_serializing_if = "Project::is_default_cache_dir"
    )]
    pub cache_dir: String,
}

impl Project {
    fn default_cache_dir() -> String {
        "build".to_string()
    }

    fn is_default_cache_dir(dir: &String) -> bool {
        *dir == Self::default().cache_dir
    }

    pub fn add_dependency(&mut self, dep: Dependency) {
        if let Some(existing) = self.dependencies.iter_mut().find(|d| d.name == dep.name) {
            *existing = dep;
        } else {
            self.dependencies.push(dep);
        }
    }
}

impl Default for Project {
    fn default() -> Self {
        Self {
            name: "sandbox".to_string(),
            dependencies: vec![],
            cache_dir: Self::default_cache_dir(),
        }
    }
}

fn serialize_dependencies<S>(deps: &[Dependency], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let dep_strings: Vec<String> = deps.iter().map(std::string::ToString::to_string).collect();
    dep_strings.serialize(serializer)
}

fn deserialize_dependencies<'de, D>(deserializer: D) -> Result<Vec<Dependency>, D::Error>
where
    D: Deserializer<'de>,
{
    let dep_strings: Vec<String> = Deserialize::deserialize(deserializer)?;
    dep_strings
        .into_iter()
        .map(|dep_str| Dependency::from_str(&dep_str).map_err(serde::de::Error::custom))
        .collect()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CMakeConfig {
    pub standard: CppStandard,
    pub export_compile_commands: bool,
}

impl Default for CMakeConfig {
    fn default() -> Self {
        Self {
            standard: CppStandard::default(),
            export_compile_commands: true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ConanConfig {
    pub bin: String,
    pub remote: Option<String>,
}

impl Default for ConanConfig {
    fn default() -> Self {
        Self {
            bin: "conan".to_string(),
            remote: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TestingConfig {
    pub dir: String,
}

impl Default for TestingConfig {
    fn default() -> Self {
        Self {
            dir: "tests".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_project_default_deserialization() -> Result<()> {
        let toml_data = r#"
        name = "example"
        dependencies = []
    "#;

        let project: Project = toml::from_str(toml_data)?;
        assert!(project.cache_dir == "build");

        Ok(())
    }

    /// Verify that old config files with removed fields (`silent`, `enabled`,
    /// `link`) still deserialize correctly.
    #[test]
    fn test_backwards_compatible_deserialization() -> Result<()> {
        let toml_data = r#"
[project]
name = "legacy"
dependencies = []

[cmake]
standard = "Cpp20"
export_compile_commands = true
silent = false

[conan]
bin = "conan"
silent = true

[testing]
enabled = true
link = false
dir = "tests"
"#;
        let config: Config = toml::from_str(toml_data)?;
        assert_eq!(config.project.name, "legacy");
        assert_eq!(config.testing.dir, "tests");
        Ok(())
    }

    #[test]
    fn test_project_add_dependency() {
        let mut project = Project::default();

        // Add first dependency
        project.add_dependency(Dependency {
            name: "DependencyA".to_string(),
            ..Default::default()
        });
        assert_eq!(project.dependencies.len(), 1);
        assert_eq!(project.dependencies[0].name, "DependencyA");

        // Add another dependency
        project.add_dependency(Dependency {
            name: "DependencyB".to_string(),
            ..Default::default()
        });
        assert_eq!(project.dependencies.len(), 2);
        assert_eq!(project.dependencies[1].name, "DependencyB");

        // Replace existing dependency
        project.add_dependency(Dependency {
            name: "DependencyA".to_string(),
            ..Default::default()
        });
        assert_eq!(project.dependencies.len(), 2); // Should not increase
        assert_eq!(project.dependencies[0].name, "DependencyA"); // Should be replaced
    }
}
