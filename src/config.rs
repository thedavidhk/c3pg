use std::{fmt::Display, str::FromStr};

use anyhow::Context;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    cmake::CppStandard,
    dependency::Dependency,
    traits::{FromFile, ToFile},
};

#[derive(Default, FromFile, ToFile, Serialize, Deserialize)]
pub struct Config {
    pub project: Project,
    pub cmake: CMakeConfig,
    pub conan: ConanConfig,
}

impl Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let contents = toml::to_string_pretty(self).expect("Could not serialize Config to toml");
        write!(f, "{}", contents)
    }
}

impl FromStr for Config {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        toml::from_str(s).context("Failed to parse the config string as TOML")
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    #[serde(
        serialize_with = "serialize_dependencies",
        deserialize_with = "deserialize_dependencies"
    )]
    pub dependencies: Vec<Dependency>,
}

fn serialize_dependencies<S>(deps: &[Dependency], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let dep_strings: Vec<String> = deps.iter().map(|dep| dep.to_string()).collect();
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

impl Project {
    pub fn add_dependency(&mut self, dep: Dependency) {
        self.dependencies.push(dep);
    }
}

#[derive(Serialize, Deserialize)]
pub struct CMakeConfig {
    pub standard: CppStandard,
    pub export_compile_commands: bool,
}

impl Default for CMakeConfig {
    fn default() -> Self {
        Self {
            standard: Default::default(),
            export_compile_commands: true,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ConanConfig {
    pub bin: String,
    pub remote: Option<String>,
}

impl Default for ConanConfig {
    fn default() -> Self {
        Self {
            bin: "conan".to_string(),
            remote: Default::default(),
        }
    }
}
