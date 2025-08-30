use anyhow::{anyhow, Result};
use heck::ToPascalCase;
use log::LevelFilter;
use semver::Version;
use std::fmt;
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

        args.extend(conan_verbosity_args(lvl).iter().map(|s| s.to_string()));

        runner
            .command(&self.bin)
            .args(args.iter().map(|s| s.as_str()))
            .stream_mode(tool_stream_mode(lvl))
            .run()?
            .expect_success("Conan install failed")?;

        Ok(())
    }

    pub fn get_latest_matching_dependency(
        &self,
        runner: impl CommandRunner,
        expr: &str,
    ) -> Result<Option<Dependency>> {
        let dependency =
            Dependency::from_str(expr).map_err(|e| anyhow!("Could not parse {}: {}", expr, e))?;
        self.find_dependency(runner, dependency)
    }

    fn find_dependency(
        &self,
        runner: impl CommandRunner,
        dependency: Dependency,
    ) -> Result<Option<Dependency>> {
        let search_result = self.search(runner, dependency.name.as_str())?;

        // collect (version, dep) pairs for those that parse as semver
        let mut pairs: Vec<(Version, Dependency)> = search_result
            .lines()
            .filter_map(|l| Dependency::from_str(l).ok())
            .filter(|d| d.matches(&dependency))
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
        let remote = command
            .stdout
            .lines()
            .next()
            .ok_or(anyhow!("remotes list is empty"))?;
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

fn conan_verbosity_args(lvl: LevelFilter) -> &'static [&'static str] {
    match lvl {
        LevelFilter::Off => &["-vquiet"],
        LevelFilter::Error => &["-vquiet"],
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
        let mock_runner = MockCommandRunner::new(Some("default_remote".to_string()));

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
