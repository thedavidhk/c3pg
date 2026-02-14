use anyhow::{bail, Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{HashMap, HashSet};
use std::{fmt, str::FromStr};

use crate::{
    cmake::CppStandard,
    dependency::Dependency,
    traits::{FromFile, ToFile},
};

#[derive(Debug, Default, FromFile, ToFile, Serialize, Deserialize)]
pub struct Config {
    pub project: Project,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<TargetConfig>,
    #[serde(default, skip_serializing_if = "CMakeConfig::is_default")]
    pub cmake: CMakeConfig,
    #[serde(default, skip_serializing_if = "ConanConfig::is_default")]
    pub conan: ConanConfig,
    #[serde(default, skip_serializing_if = "TestingConfig::is_default")]
    pub testing: TestingConfig,
}

// ---------------------------------------------------------------------------
// Target types
// ---------------------------------------------------------------------------

/// The kind of build artifact a target produces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetType {
    Executable,
    StaticLibrary,
}

/// A target as declared in `[[targets]]` in `c3pg.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TargetConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub target_type: TargetType,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_include: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link: Vec<String>,
}

/// A resolved target ready for `CMake` generation. Source entries have been
/// expanded to concrete file paths.
#[derive(Debug, Clone)]
pub struct EffectiveTarget {
    pub name: String,
    pub target_type: TargetType,
    pub source_files: Vec<String>,
    pub public_include: Vec<String>,
    pub link: Vec<String>,
}

/// Validate the `[[targets]]` array from a config file.
///
/// Checks for duplicate names, dangling `link` references, and dependency
/// cycles.
///
/// # Errors
///
/// Returns an error describing the first validation problem found.
pub fn validate_targets(targets: &[TargetConfig]) -> Result<()> {
    if targets.is_empty() {
        return Ok(());
    }

    // Duplicate names
    let mut seen = HashSet::new();
    for t in targets {
        if !seen.insert(&t.name) {
            bail!("duplicate target name: '{}'", t.name);
        }
    }

    // Dangling link references
    let names: HashSet<&str> = targets.iter().map(|t| t.name.as_str()).collect();
    for t in targets {
        for link in &t.link {
            if !names.contains(link.as_str()) {
                bail!(
                    "target '{}' links to '{}', which is not a declared target",
                    t.name,
                    link
                );
            }
        }
    }

    // Cycle detection via DFS
    let adj: HashMap<&str, &[String]> = targets
        .iter()
        .map(|t| (t.name.as_str(), t.link.as_slice()))
        .collect();

    // 0 = unvisited, 1 = in-stack, 2 = done
    let mut state: HashMap<&str, u8> = names.iter().map(|&n| (n, 0u8)).collect();

    for &start in &names {
        if state[start] == 0 {
            let mut stack = vec![(start, 0usize)]; // (node, edge index)
            state.insert(start, 1);
            while let Some((node, idx)) = stack.last_mut() {
                let edges = adj[*node];
                if *idx < edges.len() {
                    let next = edges[*idx].as_str();
                    *idx += 1;
                    match state[next] {
                        1 => bail!("dependency cycle detected involving target '{next}'"),
                        0 => {
                            state.insert(next, 1);
                            stack.push((next, 0));
                        }
                        _ => {} // already visited
                    }
                } else {
                    state.insert(node, 2);
                    stack.pop();
                }
            }
        }
    }

    Ok(())
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

    #[serde(default)]
    pub standard: CppStandard,

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
            standard: CppStandard::default(),
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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CMakeConfig {
    pub export_compile_commands: bool,
}

impl CMakeConfig {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for CMakeConfig {
    fn default() -> Self {
        Self {
            export_compile_commands: true,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConanConfig {
    pub bin: String,
    pub remote: Option<String>,
}

impl ConanConfig {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for ConanConfig {
    fn default() -> Self {
        Self {
            bin: "conan".to_string(),
            remote: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TestingConfig {
    pub dir: String,
}

impl TestingConfig {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
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

    // -----------------------------------------------------------------------
    // Target config deserialization
    // -----------------------------------------------------------------------

    #[test]
    fn test_config_without_targets_is_valid() -> Result<()> {
        let toml_data = r#"
[project]
name = "simple"
dependencies = []

[cmake]
[conan]
[testing]
"#;
        let config: Config = toml::from_str(toml_data)?;
        assert_eq!(config.project.name, "simple");
        Ok(())
    }

    #[test]
    fn test_config_with_targets_deserializes() -> Result<()> {
        let toml_data = r#"
[project]
name = "multi"
dependencies = ["fmt/11.0.0"]

[[targets]]
name = "mylib"
type = "static-library"
sources = ["src/lib"]
public-include = ["include"]

[[targets]]
name = "myapp"
type = "executable"
sources = ["src/main.cpp"]
link = ["mylib"]

[cmake]
[conan]
[testing]
"#;
        let config: Config = toml::from_str(toml_data)?;
        assert_eq!(config.targets.len(), 2);
        assert_eq!(config.targets[0].name, "mylib");
        assert_eq!(config.targets[0].target_type, TargetType::StaticLibrary);
        assert_eq!(config.targets[0].sources, vec!["src/lib"]);
        assert_eq!(config.targets[0].public_include, vec!["include"]);
        assert!(config.targets[0].link.is_empty());

        assert_eq!(config.targets[1].name, "myapp");
        assert_eq!(config.targets[1].target_type, TargetType::Executable);
        assert_eq!(config.targets[1].sources, vec!["src/main.cpp"]);
        assert!(config.targets[1].public_include.is_empty());
        assert_eq!(config.targets[1].link, vec!["mylib"]);
        Ok(())
    }

    #[test]
    fn test_config_with_targets_does_not_serialize_empty() {
        let config = Config::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(
            !serialized.contains("targets"),
            "Empty targets should not appear in serialized output"
        );
    }

    // -----------------------------------------------------------------------
    // Target validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_empty_targets_ok() {
        assert!(validate_targets(&[]).is_ok());
    }

    #[test]
    fn test_validate_valid_targets() {
        let targets = vec![
            TargetConfig {
                name: "mylib".into(),
                target_type: TargetType::StaticLibrary,
                sources: vec!["src/lib".into()],
                public_include: vec!["include".into()],
                link: vec![],
            },
            TargetConfig {
                name: "myapp".into(),
                target_type: TargetType::Executable,
                sources: vec!["src/main.cpp".into()],
                public_include: vec![],
                link: vec!["mylib".into()],
            },
        ];
        assert!(validate_targets(&targets).is_ok());
    }

    #[test]
    fn test_validate_duplicate_names() {
        let targets = vec![
            TargetConfig {
                name: "dup".into(),
                target_type: TargetType::Executable,
                sources: vec![],
                public_include: vec![],
                link: vec![],
            },
            TargetConfig {
                name: "dup".into(),
                target_type: TargetType::StaticLibrary,
                sources: vec![],
                public_include: vec![],
                link: vec![],
            },
        ];
        let err = validate_targets(&targets).unwrap_err();
        assert!(
            format!("{err}").contains("duplicate"),
            "Expected duplicate error, got: {err}"
        );
    }

    #[test]
    fn test_validate_dangling_link() {
        let targets = vec![TargetConfig {
            name: "app".into(),
            target_type: TargetType::Executable,
            sources: vec![],
            public_include: vec![],
            link: vec!["nonexistent".into()],
        }];
        let err = validate_targets(&targets).unwrap_err();
        assert!(
            format!("{err}").contains("nonexistent"),
            "Expected dangling link error, got: {err}"
        );
    }

    #[test]
    fn test_validate_cycle() {
        let targets = vec![
            TargetConfig {
                name: "a".into(),
                target_type: TargetType::StaticLibrary,
                sources: vec![],
                public_include: vec![],
                link: vec!["b".into()],
            },
            TargetConfig {
                name: "b".into(),
                target_type: TargetType::StaticLibrary,
                sources: vec![],
                public_include: vec![],
                link: vec!["a".into()],
            },
        ];
        let err = validate_targets(&targets).unwrap_err();
        assert!(
            format!("{err}").contains("cycle"),
            "Expected cycle error, got: {err}"
        );
    }

    #[test]
    fn test_validate_self_link_is_cycle() {
        let targets = vec![TargetConfig {
            name: "self".into(),
            target_type: TargetType::StaticLibrary,
            sources: vec![],
            public_include: vec![],
            link: vec!["self".into()],
        }];
        let err = validate_targets(&targets).unwrap_err();
        assert!(
            format!("{err}").contains("cycle"),
            "Expected cycle error, got: {err}"
        );
    }
}
