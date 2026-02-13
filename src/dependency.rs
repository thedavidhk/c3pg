use anyhow::anyhow;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr, sync::OnceLock};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    pub version: Option<String>,
    pub channel: Option<String>,
}

impl Dependency {
    #[must_use] 
    pub fn matches(&self, other: &Dependency) -> bool {
        self.name == other.name
            && (self.version.is_none() || other.version.is_none() || self.version == other.version)
            && (self.channel.is_none() || other.channel.is_none() || self.channel == other.channel)
    }
}

impl FromStr for Dependency {
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(
                r"^\s*(?P<name>[^/]+)(?:/(?P<version>[^@#:]+))?(?:@(?P<channel>[^#:]+))?(?::|#)?.*$",
            )
            .expect("Could not create regex (this should not happen).")
        });

        let captures = re.captures(s).ok_or_else(|| anyhow!("Did not match"))?;

        let name = captures
            .name("name")
            .ok_or_else(|| anyhow!("No name in match"))?
            .as_str()
            .to_string();

        let version = captures.name("version").map(|val| val.as_str().to_string());
        let channel = captures.name("channel").map(|val| val.as_str().to_string());

        Ok(Self {
            name,
            version,
            channel,
        })
    }

    type Err = anyhow::Error;
}

impl fmt::Display for Dependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        if let Some(version) = &self.version {
            write!(f, "/{version}")?;
        }
        if let Some(channel) = &self.channel {
            write!(f, "@{channel}")?;
        }
        Ok(())
    }
}
