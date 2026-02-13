use anyhow::{anyhow, Result};
use heck::ToPascalCase;
use log::LevelFilter;
use semver::Version;
use std::fmt;
use std::path::Path;
use std::str::FromStr;

use crate::command_runner::{tool_stream_mode, CommandRunner};
use crate::config::Config;
use crate::dependency::Dependency;
use crate::traits::ToFile;

#[derive(Debug, ToFile)]
pub struct Conan {
    bin: String,
    remote: String,
    project_name: String,
    requirements: Vec<Dependency>,
}

impl Conan {
    /// Build a [`Conan`] instance from the project configuration.
    ///
    /// If no explicit remote is set in `config`, the first remote reported
    /// by `conan remote list` is used.
    ///
    /// # Errors
    ///
    /// Returns an error if no Conan remote is configured and `conan remote
    /// list` fails or returns an empty list.
    pub fn from_config(runner: impl CommandRunner, config: &Config) -> Result<Self> {
        Ok(Self {
            bin: config.conan.bin.clone(),
            remote: config
                .conan
                .remote
                .clone()
                .unwrap_or(Self::get_first_remote(runner, "conan")?),
            requirements: config.project.dependencies.clone(),
            project_name: config.project.name.clone(),
        })
    }

    /// Run `conan install` for the project in `dir`, writing outputs to
    /// `out_dir`.
    ///
    /// # Errors
    ///
    /// Returns an error if the `conan install` command fails (e.g. a
    /// dependency cannot be built or downloaded).
    pub fn install(
        &self,
        runner: &impl CommandRunner,
        dir: &str,
        out_dir: &str,
        build_type: crate::cmake::BuildType,
        lvl: LevelFilter,
    ) -> Result<()> {
        let mut args: Vec<String> = vec![
            "install".into(),
            "--build=missing".into(),
            "-s".into(),
            format!("build_type={}", build_type),
            "--output-folder".into(),
            out_dir.into(),
            dir.into(),
        ];

        args.extend(conan_verbosity_args(lvl).iter().map(std::string::ToString::to_string));

        runner
            .command(&self.bin)
            .args(args.iter().map(std::string::String::as_str))
            .stream_mode(tool_stream_mode(lvl))
            .run()?
            .expect_success("Conan install failed")?;

        Ok(())
    }

    /// Search the configured remote for the latest stable version matching
    /// `expr` and return it, or `None` if no match is found.
    ///
    /// # Errors
    ///
    /// Returns an error if `expr` cannot be parsed as a dependency
    /// specifier or the `conan search` command fails.
    pub fn get_latest_matching_dependency(
        &self,
        runner: impl CommandRunner,
        expr: &str,
    ) -> Result<Option<Dependency>> {
        let dependency =
            Dependency::from_str(expr).map_err(|e| anyhow!("Could not parse {}: {}", expr, e))?;
        self.find_dependency(runner, &dependency)
    }

    fn find_dependency(
        &self,
        runner: impl CommandRunner,
        dependency: &Dependency,
    ) -> Result<Option<Dependency>> {
        let search_result = self.search(runner, dependency.name.as_str())?;

        // collect (version, dep) pairs for those that parse as semver
        let mut pairs: Vec<(Version, Dependency)> = search_result
            .lines()
            .filter_map(|l| Dependency::from_str(l).ok())
            .filter(|d| d.matches(dependency))
            .filter_map(|d| {
                // adjust if your version access differs
                Version::parse(d.version.clone()?.as_str())
                    .ok()
                    .map(|v| (v, d))
            })
            .collect();

        pairs.sort_unstable_by(|a, b| a.0.cmp(&b.0)); // ascending

        // prefer stable; otherwise take highest pre-release
        let picked = pairs
            .iter()
            .rev()
            .find(|(v, _)| v.pre.is_empty())
            .cloned()
            .or_else(|| pairs.into_iter().next_back());

        Ok(picked.map(|(_, d)| d))
    }

    fn search(&self, runner: impl CommandRunner, expr: &str) -> Result<String> {
        let output = runner
            .command(&self.bin)
            .args(["search", "-r", &self.remote, expr])
            .run()?
            .expect_success_with_stdout(
                format!("Could not search in remote {}", &self.remote).as_str(),
            )?;
        Ok(output)
    }

    fn get_first_remote(runner: impl CommandRunner, bin: &str) -> Result<String> {
        let command = runner.command(bin).args(["remote", "list"]).run()?;
        let remote_name = command
            .stdout
            .lines()
            .find(|l| !l.trim().is_empty())
            .ok_or(anyhow!(
                "no Conan remotes configured -- run `conan remote add` first"
            ))?
            .split_once(':')
            .map(|(name, _)| name.trim().to_string())
            .ok_or(anyhow!("unexpected Conan remote list format"))?;
        Ok(remote_name)
    }
}

impl fmt::Display for Conan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let requirements = if self.requirements.is_empty() {
            "        pass".to_string()
        } else {
            self.requirements
                .iter()
                .map(|dep| format!("        self.requires(\"{dep}\")"))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let class_name = self.project_name.to_pascal_case();

        write!(
            f,
            r#"from conan import ConanFile


class {class_name}(ConanFile):
    name = "{project_name}"
    version = "0.1"
    settings = "os", "compiler", "build_type", "arch"
    generators = "CMakeDeps", "CMakeToolchain"

    def requirements(self):
{requirements}
"#,
            requirements = requirements,
            class_name = class_name,
            project_name = self.project_name,
        )
    }
}

/// Parse Conan-generated build-environment scripts in `cache_dir` and return
/// any compiler-related environment variables (`CC`, `CXX`, etc.) as
/// `(key, value)` pairs.
///
/// On Unix, this reads `conanbuildenv-*.sh` files and parses `export VAR=val`
/// lines.  On Windows, it reads `conanbuildenv-*.bat` files and parses
/// `set "VAR=val"` lines.
///
/// These are typically needed so cmake picks up the same compiler that
/// Conan was configured for.
#[must_use]
pub fn parse_conan_build_env(cache_dir: &Path) -> Vec<(String, String)> {
    use std::fs;

    let mut env = Vec::new();

    let Ok(entries) = fs::read_dir(cache_dir) else {
        return env;
    };

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let is_env_script = if cfg!(windows) {
            name.starts_with("conanbuildenv-") && name.ends_with(".bat")
        } else {
            name.starts_with("conanbuildenv-") && name.ends_with(".sh")
        };
        if !is_env_script {
            continue;
        }
        let Ok(contents) = fs::read_to_string(entry.path()) else {
            continue;
        };
        for line in contents.lines() {
            let line = line.trim();
            if cfg!(windows) {
                // Match lines like: set "CC=cl.exe"
                if let Some(rest) = line.strip_prefix("set \"") {
                    if let Some(rest) = rest.strip_suffix('"') {
                        if let Some((key, val)) = rest.split_once('=') {
                            env.push((key.to_string(), val.to_string()));
                        }
                    }
                }
            } else {
                // Match lines like: export CC="clang"  or  export CXX=clang++
                if let Some(rest) = line.strip_prefix("export ") {
                    if let Some((key, val)) = rest.split_once('=') {
                        let val = val.trim_matches('"').trim_matches('\'');
                        env.push((key.to_string(), val.to_string()));
                    }
                }
            }
        }
    }

    env
}

fn conan_verbosity_args(lvl: LevelFilter) -> &'static [&'static str] {
    match lvl {
        LevelFilter::Off | LevelFilter::Error => &["-vquiet"],
        LevelFilter::Warn => &["-verror"],
        LevelFilter::Info => &["-vwarning"],
        LevelFilter::Debug => &["-vstatus"],
        LevelFilter::Trace => &["-v"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{cmake::BuildType, test_utils::MockCommandRunner};

    #[test]
    fn test_conan_from_config_with_remote_fallback() {
        let mock_runner =
            MockCommandRunner::new(Some("default_remote: https://example.com [Enabled]".to_string()));

        let config = Config {
            project: crate::config::Project {
                name: "TestProject".to_string(),
                dependencies: vec![Dependency {
                    name: "TestDependency".to_string(),
                    ..Default::default()
                }],
                cache_dir: "build".to_string(),
            },
            cmake: crate::config::CMakeConfig::default(),
            conan: crate::config::ConanConfig::default(),
            testing: crate::config::TestingConfig::default(),
        };

        let conan =
            Conan::from_config(&mock_runner, &config).expect("Failed to create Conan instance");

        assert_eq!(conan.bin, "conan");
        assert_eq!(conan.remote, "default_remote");
        assert_eq!(conan.requirements.len(), 1);
        assert_eq!(conan.requirements[0].name, "TestDependency");

        // Ensure `get_first_remote` was called
        let commands = mock_runner.executed_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].0, "conan");
        assert_eq!(commands[0].1, vec!["remote", "list"]);
    }

    #[test]
    fn test_conan_install() {
        let mock_runner = MockCommandRunner::new(Some(String::new()));

        let conan = Conan {
            bin: "conan".to_string(),
            remote: "default_remote".to_string(),
            requirements: vec![Dependency {
                name: "TestDependency".to_string(),
                ..Default::default()
            }],
            project_name: "example".to_string(),
        };

        conan
            .install(
                &mock_runner,
                "source_dir",
                "build_dir",
                BuildType::Debug,
                LevelFilter::Info,
            )
            .expect("Conan install failed");

        let commands = mock_runner.executed_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].0, "conan");
        assert_eq!(
            commands[0].1,
            vec![
                "install",
                "--build=missing",
                "-s",
                "build_type=Debug",
                "--output-folder",
                "build_dir",
                "source_dir",
                "-vwarning"
            ]
        );
    }

    #[test]
    fn test_conan_search() {
        let mock_runner = MockCommandRunner::new(Some(
            "TestDependency/1.0.0\nTestDependency/1.1.0".to_string(),
        ));

        let conan = Conan {
            bin: "conan".to_string(),
            remote: "default_remote".to_string(),
            project_name: "example".to_string(),
            requirements: vec![],
        };

        let result = conan
            .search(&mock_runner, "TestDependency")
            .expect("Conan search failed");

        assert_eq!(result, "TestDependency/1.0.0\nTestDependency/1.1.0");

        let commands = mock_runner.executed_commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].0, "conan");
        assert_eq!(
            commands[0].1,
            vec!["search", "-r", "default_remote", "TestDependency"]
        );
    }

    #[test]
    fn test_conan_fmt_empty_requirements() {
        let conan = Conan {
            bin: "conan".to_string(),
            remote: "default_remote".to_string(),
            project_name: "example".to_string(),
            requirements: vec![],
        };

        let formatted = conan.to_string();
        assert!(formatted.contains("def requirements(self):"));
        assert!(formatted.contains("pass"));
    }

    #[test]
    fn test_conan_fmt_with_requirements() {
        let conan = Conan {
            bin: "conan".to_string(),
            remote: "default_remote".to_string(),
            project_name: "example".to_string(),
            requirements: vec![
                Dependency {
                    name: "Dep1".to_string(),
                    ..Default::default()
                },
                Dependency {
                    name: "Dep2".to_string(),
                    ..Default::default()
                },
            ],
        };

        let formatted = conan.to_string();
        assert!(formatted.contains("def requirements(self):"));
        assert!(formatted.contains("self.requires(\"Dep1\")"));
        assert!(formatted.contains("self.requires(\"Dep2\")"));
    }
}
