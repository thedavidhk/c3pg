use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::{fmt, str::FromStr};

use crate::{
    cmake::CppStandard,
    dependency::Dependency,
    traits::{FromFile, ToFile},
};

// ---------------------------------------------------------------------------
// Dependency value (TOML representation)
// ---------------------------------------------------------------------------

/// How a single dependency is represented in `[dependencies]`.
///
/// Simple form: `name = "version"`
/// Detailed form: `name = { version = "...", user = "...", channel = "..." }`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DependencyValue {
    Simple(String),
    Detailed {
        version: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        user: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
    },
}

impl DependencyValue {
    /// Convert an internal [`Dependency`] to its TOML representation.
    ///
    /// Uses the simple form when only a version is present, and the detailed
    /// form when `user` or `channel` are set.
    #[must_use]
    pub fn from_dependency(dep: &Dependency) -> Self {
        if dep.user.is_some() || dep.channel.is_some() {
            Self::Detailed {
                version: dep.version.clone().unwrap_or_default(),
                user: dep.user.clone(),
                channel: dep.channel.clone(),
            }
        } else {
            Self::Simple(dep.version.clone().unwrap_or_default())
        }
    }

    /// Convert back to an internal [`Dependency`] given the package name.
    #[must_use]
    pub fn to_dependency(&self, name: &str) -> Dependency {
        match self {
            Self::Simple(version) => Dependency {
                name: name.to_string(),
                version: Some(version.clone()),
                user: None,
                channel: None,
            },
            Self::Detailed {
                version,
                user,
                channel,
            } => Dependency {
                name: name.to_string(),
                version: Some(version.clone()),
                user: user.clone(),
                channel: channel.clone(),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level config
// ---------------------------------------------------------------------------

#[derive(Debug, Default, FromFile, ToFile, Serialize, Deserialize)]
pub struct Config {
    pub project: Project,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dependencies: BTreeMap<String, DependencyValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lib: Vec<LibConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bin: Vec<BinConfig>,
    #[serde(default, skip_serializing_if = "CMakeConfig::is_default")]
    pub cmake: CMakeConfig,
    #[serde(default, skip_serializing_if = "ConanConfig::is_default")]
    pub conan: ConanConfig,
    #[serde(default, skip_serializing_if = "TestingConfig::is_default")]
    pub testing: TestingConfig,
}

impl Config {
    /// Return the dependency map as a `Vec<Dependency>` for internal use
    /// (e.g. Conan requirements generation).
    #[must_use]
    pub fn dependency_vec(&self) -> Vec<Dependency> {
        self.dependencies
            .iter()
            .map(|(name, val)| val.to_dependency(name))
            .collect()
    }

    /// Insert or replace a dependency.
    pub fn add_dependency(&mut self, dep: &Dependency) {
        let value = DependencyValue::from_dependency(dep);
        self.dependencies.insert(dep.name.clone(), value);
    }

    /// Returns `true` if a dependency with the given name exists.
    #[must_use]
    pub fn has_dependency(&self, name: &str) -> bool {
        self.dependencies.contains_key(name)
    }
}

// ---------------------------------------------------------------------------
// Target types
// ---------------------------------------------------------------------------

/// The kind of build artifact a target produces (internal representation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetType {
    Executable,
    StaticLibrary,
}

/// A library target declared in `[[lib]]` in `c3pg.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LibConfig {
    pub name: String,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_include: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link: Vec<String>,
}

/// An executable target declared in `[[bin]]` in `c3pg.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BinConfig {
    pub name: String,
    #[serde(default)]
    pub sources: Vec<String>,
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

/// Validate the `[[lib]]` and `[[bin]]` arrays from a config file.
///
/// Checks for duplicate names (across both lists), dangling `link` references
/// (must point to a declared `[[lib]]`), and dependency cycles among libraries.
///
/// # Errors
///
/// Returns an error describing the first validation problem found.
pub fn validate_targets(libs: &[LibConfig], bins: &[BinConfig]) -> Result<()> {
    if libs.is_empty() && bins.is_empty() {
        return Ok(());
    }

    // Duplicate names (across both lists)
    let mut seen = HashSet::new();
    for lib in libs {
        if !seen.insert(&lib.name) {
            bail!("duplicate target name: '{}'", lib.name);
        }
    }
    for bin in bins {
        if !seen.insert(&bin.name) {
            bail!("duplicate target name: '{}'", bin.name);
        }
    }

    // Link references must point to a declared library target
    let lib_names: HashSet<&str> = libs.iter().map(|l| l.name.as_str()).collect();
    for lib in libs {
        for link in &lib.link {
            if !lib_names.contains(link.as_str()) {
                bail!(
                    "library '{}' links to '{}', which is not a declared [[lib]] target",
                    lib.name,
                    link
                );
            }
        }
    }
    for bin in bins {
        for link in &bin.link {
            if !lib_names.contains(link.as_str()) {
                bail!(
                    "executable '{}' links to '{}', which is not a declared [[lib]] target",
                    bin.name,
                    link
                );
            }
        }
    }

    // Cycle detection via DFS (only among library targets)
    let adj: HashMap<&str, &[String]> = libs
        .iter()
        .map(|l| (l.name.as_str(), l.link.as_slice()))
        .collect();

    // 0 = unvisited, 1 = in-stack, 2 = done
    let mut state: HashMap<&str, u8> = lib_names.iter().map(|&n| (n, 0u8)).collect();

    for &start in &lib_names {
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
        let raw = toml::to_string_pretty(self).expect("Could not serialize Config to toml");
        let contents = inline_dependency_subtables(&raw);
        write!(f, "{contents}")
    }
}

/// Post-process TOML output so `[dependencies.X]` subtables become inline
/// tables under a single `[dependencies]` heading.
///
/// ```text
/// [dependencies]
/// boost = "1.88"
///
/// [dependencies.mylib]
/// version = "1.0.0"
/// user = "probe"
/// channel = "release"
/// ```
/// becomes:
/// ```text
/// [dependencies]
/// boost = "1.88"
/// mylib = { version = "1.0.0", user = "probe", channel = "release" }
/// ```
fn inline_dependency_subtables(toml_str: &str) -> String {
    // First pass: collect all inline entries from [dependencies.X] subtables
    // and the byte ranges they occupy so we can skip them in the second pass.
    let mut inline_entries: Vec<String> = Vec::new();
    let mut skip_ranges: Vec<(usize, usize)> = Vec::new();
    let mut current_dep_name: Option<String> = None;
    let mut current_kv_pairs: Vec<String> = Vec::new();
    let mut subtable_start: usize = 0;

    let lines: Vec<&str> = toml_str.lines().collect();
    let mut offset = 0;

    for (i, &line) in lines.iter().enumerate() {
        let line_start = offset;
        // +1 for the '\n' (except possibly the last line)
        offset += line.len() + usize::from(i < lines.len() - 1);
        let trimmed = line.trim();

        // Detect `[dependencies.X]` subtable headers
        if let Some(rest) = trimmed.strip_prefix("[dependencies.") {
            if let Some(name) = rest.strip_suffix(']') {
                // Flush previous subtable if any
                if let Some(dep_name) = current_dep_name.take() {
                    inline_entries.push(format!(
                        "{dep_name} = {{ {} }}",
                        current_kv_pairs.join(", ")
                    ));
                    current_kv_pairs.clear();
                    skip_ranges.push((subtable_start, line_start));
                }
                current_dep_name = Some(name.to_string());
                subtable_start = line_start;
                continue;
            }
        }

        // Inside a subtable: collect kv pairs or detect end
        if current_dep_name.is_some() {
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('[') {
                // End of subtable
                if let Some(dep_name) = current_dep_name.take() {
                    inline_entries.push(format!(
                        "{dep_name} = {{ {} }}",
                        current_kv_pairs.join(", ")
                    ));
                    current_kv_pairs.clear();
                    skip_ranges.push((subtable_start, line_start));
                }
                // Fall through to handle this line normally
            } else {
                current_kv_pairs.push(trimmed.to_string());
            }
        }
    }

    // Flush final subtable
    if let Some(dep_name) = current_dep_name.take() {
        inline_entries.push(format!(
            "{dep_name} = {{ {} }}",
            current_kv_pairs.join(", ")
        ));
        skip_ranges.push((subtable_start, toml_str.len()));
    }

    // If no subtables were found, return original string as-is
    if inline_entries.is_empty() {
        return toml_str.to_string();
    }

    // Second pass: rebuild the string, skipping subtable ranges and
    // inserting inline entries at the right location.
    let mut output = String::with_capacity(toml_str.len());
    let mut pos = 0;
    let mut inlines_inserted = false;

    // Find the insertion point: end of the [dependencies] simple entries,
    // i.e. just before the first skip range.
    let insert_before = skip_ranges[0].0;

    for &(start, end) in &skip_ranges {
        // Copy everything before this skip range
        if pos < start {
            let chunk = &toml_str[pos..start];
            // If we haven't inserted inlines yet and we've reached the
            // insertion point, do it now.
            if !inlines_inserted && pos <= insert_before && start >= insert_before {
                // Output up to insertion point
                let pre = &toml_str[pos..insert_before];
                output.push_str(pre.trim_end_matches('\n'));
                output.push('\n');

                // If there's no [dependencies] header yet (all deps were
                // subtables), add one.
                if !output.contains("\n[dependencies]\n") && !output.starts_with("[dependencies]\n")
                {
                    output.push_str("[dependencies]\n");
                }

                for entry in &inline_entries {
                    output.push_str(entry);
                    output.push('\n');
                }
                output.push('\n');
                inlines_inserted = true;
            } else {
                output.push_str(chunk);
            }
        }
        pos = end;
    }

    // Copy remaining content after the last skip range
    if pos < toml_str.len() {
        output.push_str(&toml_str[pos..]);
    }

    // If we still haven't inserted (shouldn't happen but be safe)
    if !inlines_inserted {
        if !output.contains("[dependencies]") {
            output.push_str("\n[dependencies]\n");
        }
        for entry in &inline_entries {
            output.push_str(entry);
            output.push('\n');
        }
    }

    // Clean up trailing blank lines
    let trimmed = output.trim_end();
    let mut result = trimmed.to_string();
    result.push('\n');
    result
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
}

impl Default for Project {
    fn default() -> Self {
        Self {
            name: "sandbox".to_string(),
            standard: CppStandard::default(),
            cache_dir: Self::default_cache_dir(),
        }
    }
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
    fn test_config_add_dependency() {
        let mut config = Config::default();

        // Add first dependency
        config.add_dependency(&Dependency {
            name: "DependencyA".to_string(),
            version: Some("1.0".to_string()),
            ..Default::default()
        });
        assert_eq!(config.dependencies.len(), 1);
        assert!(config.dependencies.contains_key("DependencyA"));

        // Add another dependency
        config.add_dependency(&Dependency {
            name: "DependencyB".to_string(),
            version: Some("2.0".to_string()),
            ..Default::default()
        });
        assert_eq!(config.dependencies.len(), 2);
        assert!(config.dependencies.contains_key("DependencyB"));

        // Replace existing dependency
        config.add_dependency(&Dependency {
            name: "DependencyA".to_string(),
            version: Some("1.1".to_string()),
            ..Default::default()
        });
        assert_eq!(config.dependencies.len(), 2); // Should not increase
        assert_eq!(
            config.dependencies["DependencyA"],
            DependencyValue::Simple("1.1".to_string())
        );
    }

    #[test]
    fn test_dependency_value_simple_roundtrip() {
        let dep = Dependency {
            name: "fmt".to_string(),
            version: Some("11.0.0".to_string()),
            user: None,
            channel: None,
        };
        let val = DependencyValue::from_dependency(&dep);
        assert_eq!(val, DependencyValue::Simple("11.0.0".to_string()));

        let back = val.to_dependency("fmt");
        assert_eq!(back.name, "fmt");
        assert_eq!(back.version.as_deref(), Some("11.0.0"));
        assert!(back.user.is_none());
        assert!(back.channel.is_none());
    }

    #[test]
    fn test_dependency_value_detailed_roundtrip() {
        let dep = Dependency {
            name: "mylib".to_string(),
            version: Some("1.0.0".to_string()),
            user: Some("probe".to_string()),
            channel: Some("release".to_string()),
        };
        let val = DependencyValue::from_dependency(&dep);
        assert!(matches!(val, DependencyValue::Detailed { .. }));

        let back = val.to_dependency("mylib");
        assert_eq!(back.name, "mylib");
        assert_eq!(back.version.as_deref(), Some("1.0.0"));
        assert_eq!(back.user.as_deref(), Some("probe"));
        assert_eq!(back.channel.as_deref(), Some("release"));
    }

    #[test]
    fn test_config_dependencies_deserialize_simple() -> Result<()> {
        let toml_data = r#"
[project]
name = "myapp"

[dependencies]
boost = "1.88"
fmt = "11.0.0"
"#;
        let config: Config = toml::from_str(toml_data)?;
        assert_eq!(config.dependencies.len(), 2);
        assert_eq!(
            config.dependencies["boost"],
            DependencyValue::Simple("1.88".to_string())
        );
        assert_eq!(
            config.dependencies["fmt"],
            DependencyValue::Simple("11.0.0".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_config_dependencies_deserialize_detailed() -> Result<()> {
        let toml_data = r#"
[project]
name = "myapp"

[dependencies]
boost = "1.88"
mylib = { version = "1.0.0", user = "probe", channel = "release" }
"#;
        let config: Config = toml::from_str(toml_data)?;
        assert_eq!(config.dependencies.len(), 2);
        assert_eq!(
            config.dependencies["boost"],
            DependencyValue::Simple("1.88".to_string())
        );
        assert_eq!(
            config.dependencies["mylib"],
            DependencyValue::Detailed {
                version: "1.0.0".to_string(),
                user: Some("probe".to_string()),
                channel: Some("release".to_string()),
            }
        );
        Ok(())
    }

    #[test]
    fn test_config_dependencies_serialize_roundtrip() -> Result<()> {
        let mut config = Config::default();
        config.project.name = "myapp".to_string();
        config.add_dependency(&Dependency {
            name: "boost".to_string(),
            version: Some("1.88".to_string()),
            ..Default::default()
        });
        config.add_dependency(&Dependency {
            name: "mylib".to_string(),
            version: Some("1.0.0".to_string()),
            user: Some("probe".to_string()),
            channel: Some("release".to_string()),
        });

        let serialized = config.to_string();

        // Simple dependency should serialize as just a version string
        assert!(
            serialized.contains("boost = \"1.88\""),
            "Expected simple dep format, got:\n{serialized}"
        );

        // Detailed dependency should be an inline table (not a subtable)
        assert!(
            serialized.contains(
                "mylib = { version = \"1.0.0\", user = \"probe\", channel = \"release\" }"
            ),
            "Expected inline table format for detailed dep, got:\n{serialized}"
        );
        assert!(
            !serialized.contains("[dependencies.mylib]"),
            "Should not use subtable format, got:\n{serialized}"
        );

        // Roundtrip: parse back and verify
        let reparsed: Config = toml::from_str(&serialized)?;
        assert_eq!(reparsed.dependencies.len(), 2);
        assert_eq!(
            reparsed.dependencies["boost"],
            DependencyValue::Simple("1.88".to_string())
        );

        let mylib_dep = reparsed.dependencies["mylib"].to_dependency("mylib");
        assert_eq!(mylib_dep.version.as_deref(), Some("1.0.0"));
        assert_eq!(mylib_dep.user.as_deref(), Some("probe"));
        assert_eq!(mylib_dep.channel.as_deref(), Some("release"));

        Ok(())
    }

    #[test]
    fn test_config_serialize_only_detailed_deps() -> Result<()> {
        let mut config = Config::default();
        config.project.name = "myapp".to_string();
        config.add_dependency(&Dependency {
            name: "mylib".to_string(),
            version: Some("1.0.0".to_string()),
            user: Some("probe".to_string()),
            channel: Some("release".to_string()),
        });

        let serialized = config.to_string();

        // Should have a [dependencies] header and an inline table
        assert!(
            serialized.contains("[dependencies]"),
            "Expected [dependencies] header, got:\n{serialized}"
        );
        assert!(
            serialized.contains(
                "mylib = { version = \"1.0.0\", user = \"probe\", channel = \"release\" }"
            ),
            "Expected inline table, got:\n{serialized}"
        );
        assert!(
            !serialized.contains("[dependencies.mylib]"),
            "Should not use subtable format, got:\n{serialized}"
        );

        // Roundtrip
        let reparsed: Config = toml::from_str(&serialized)?;
        assert_eq!(reparsed.dependencies.len(), 1);
        let dep = reparsed.dependencies["mylib"].to_dependency("mylib");
        assert_eq!(dep.user.as_deref(), Some("probe"));
        assert_eq!(dep.channel.as_deref(), Some("release"));

        Ok(())
    }

    #[test]
    fn test_config_no_dependencies_section_is_valid() -> Result<()> {
        let toml_data = r#"
[project]
name = "simple"
"#;
        let config: Config = toml::from_str(toml_data)?;
        assert_eq!(config.project.name, "simple");
        assert!(config.dependencies.is_empty());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Target config deserialization
    // -----------------------------------------------------------------------

    #[test]
    fn test_config_without_targets_is_valid() -> Result<()> {
        let toml_data = r#"
[project]
name = "simple"
"#;
        let config: Config = toml::from_str(toml_data)?;
        assert_eq!(config.project.name, "simple");
        assert!(config.lib.is_empty());
        assert!(config.bin.is_empty());
        Ok(())
    }

    #[test]
    fn test_config_with_targets_deserializes() -> Result<()> {
        let toml_data = r#"
[project]
name = "multi"

[dependencies]
fmt = "11.0.0"

[[lib]]
name = "mylib"
sources = ["src/lib"]
public-include = ["include"]

[[bin]]
name = "myapp"
sources = ["src/main.cpp"]
link = ["mylib"]
"#;
        let config: Config = toml::from_str(toml_data)?;
        assert_eq!(config.lib.len(), 1);
        assert_eq!(config.lib[0].name, "mylib");
        assert_eq!(config.lib[0].sources, vec!["src/lib"]);
        assert_eq!(config.lib[0].public_include, vec!["include"]);
        assert!(config.lib[0].link.is_empty());

        assert_eq!(config.bin.len(), 1);
        assert_eq!(config.bin[0].name, "myapp");
        assert_eq!(config.bin[0].sources, vec!["src/main.cpp"]);
        assert_eq!(config.bin[0].link, vec!["mylib"]);

        // Check the dependency was loaded
        assert_eq!(config.dependencies.len(), 1);
        assert!(config.dependencies.contains_key("fmt"));
        Ok(())
    }

    #[test]
    fn test_config_with_targets_does_not_serialize_empty() {
        let config = Config::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(
            !serialized.contains("[[lib]]") && !serialized.contains("[[bin]]"),
            "Empty lib/bin should not appear in serialized output"
        );
    }

    #[test]
    fn test_config_empty_dependencies_not_serialized() {
        let config = Config::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        assert!(
            !serialized.contains("[dependencies]"),
            "Empty dependencies should not appear in serialized output"
        );
    }

    // -----------------------------------------------------------------------
    // Target validation
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_empty_targets_ok() {
        assert!(validate_targets(&[], &[]).is_ok());
    }

    #[test]
    fn test_validate_valid_targets() {
        let libs = vec![LibConfig {
            name: "mylib".into(),
            sources: vec!["src/lib".into()],
            public_include: vec!["include".into()],
            link: vec![],
        }];
        let bins = vec![BinConfig {
            name: "myapp".into(),
            sources: vec!["src/main.cpp".into()],
            link: vec!["mylib".into()],
        }];
        assert!(validate_targets(&libs, &bins).is_ok());
    }

    #[test]
    fn test_validate_duplicate_names() {
        let libs = vec![LibConfig {
            name: "dup".into(),
            sources: vec![],
            public_include: vec![],
            link: vec![],
        }];
        let bins = vec![BinConfig {
            name: "dup".into(),
            sources: vec![],
            link: vec![],
        }];
        let err = validate_targets(&libs, &bins).unwrap_err();
        assert!(
            format!("{err}").contains("duplicate"),
            "Expected duplicate error, got: {err}"
        );
    }

    #[test]
    fn test_validate_dangling_link() {
        let bins = vec![BinConfig {
            name: "app".into(),
            sources: vec![],
            link: vec!["nonexistent".into()],
        }];
        let err = validate_targets(&[], &bins).unwrap_err();
        assert!(
            format!("{err}").contains("nonexistent"),
            "Expected dangling link error, got: {err}"
        );
    }

    #[test]
    fn test_validate_cycle() {
        let libs = vec![
            LibConfig {
                name: "a".into(),
                sources: vec![],
                public_include: vec![],
                link: vec!["b".into()],
            },
            LibConfig {
                name: "b".into(),
                sources: vec![],
                public_include: vec![],
                link: vec!["a".into()],
            },
        ];
        let err = validate_targets(&libs, &[]).unwrap_err();
        assert!(
            format!("{err}").contains("cycle"),
            "Expected cycle error, got: {err}"
        );
    }

    #[test]
    fn test_validate_self_link_is_cycle() {
        let libs = vec![LibConfig {
            name: "self".into(),
            sources: vec![],
            public_include: vec![],
            link: vec!["self".into()],
        }];
        let err = validate_targets(&libs, &[]).unwrap_err();
        assert!(
            format!("{err}").contains("cycle"),
            "Expected cycle error, got: {err}"
        );
    }
}
