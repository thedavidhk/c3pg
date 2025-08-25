pub trait CommandRunner: Sized {
    /// Executes a command with the specified arguments and returns a `CommandOutput`.
    /// This is intended to be used internally by `CommandBuilder`.
    fn execute(&self, cmd: &str, args: &[&str]) -> anyhow::Result<CommandResult>;

    /// Creates a `CommandBuilder` for constructing and running a command.
    fn command(&self, cmd: impl Into<String>) -> CommandBuilder<&Self> {
        CommandBuilder::new(self, cmd)
    }
}

impl<T: CommandRunner> CommandRunner for &T {
    fn execute(&self, cmd: &str, args: &[&str]) -> anyhow::Result<CommandResult> {
        (*self).execute(cmd, args)
    }
}

pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn execute(&self, cmd: &str, args: &[&str]) -> anyhow::Result<CommandResult> {
        let output = std::process::Command::new(cmd).args(args).output()?;
        Ok(CommandResult::from_std(output))
    }
}

#[derive(Debug)]
pub struct CommandBuilder<R: CommandRunner> {
    runner: R,
    cmd: String,
    args: Vec<String>,
}

impl<R: CommandRunner> CommandBuilder<R> {
    pub fn new(runner: R, cmd: impl Into<String>) -> Self {
        Self {
            runner,
            cmd: cmd.into(),
            args: Vec::new(),
        }
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(|arg| arg.into()));
        self
    }

    pub fn run(self) -> anyhow::Result<CommandResult> {
        let args: Vec<_> = self.args.iter().map(String::as_str).collect();
        self.runner.execute(&self.cmd, &args)
    }
}

#[derive(Debug)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

impl CommandResult {
    /// Create a `CommandOutput` from `std::process::Output`.
    pub fn from_std(output: std::process::Output) -> Self {
        Self {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
        }
    }

    /// Returns the `stdout` if the command succeeded, or an error otherwise.
    pub fn expect_success_with_stdout(self, cmd: &str) -> anyhow::Result<String> {
        if self.success {
            let stderr = if self.stderr.is_empty() {
                String::new()
            } else {
                format!("{}\n", self.stderr)
            };
            Ok(format!("{}\n{}", self.stdout, stderr))
        } else {
            anyhow::bail!("Command `{}` failed: {}", cmd, self.stderr);
        }
    }

    /// Ensures the command succeeded. Otherwise, returns an error.
    pub fn expect_success(self, error_msg: &str) -> anyhow::Result<()> {
        if self.success {
            Ok(())
        } else {
            anyhow::bail!("{}: {}", error_msg, self.stderr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockRunner;

    impl CommandRunner for MockRunner {
        fn execute(&self, cmd: &str, args: &[&str]) -> anyhow::Result<CommandResult> {
            match cmd {
                "echo" => Ok(CommandResult {
                    stdout: args.join(" "),
                    stderr: String::new(),
                    success: true,
                }),
                "fail" => Ok(CommandResult {
                    stdout: String::new(),
                    stderr: "Something went wrong!".to_string(),
                    success: false,
                }),
                _ => anyhow::bail!("Unknown command"),
            }
        }
    }

    #[test]
    fn test_command_builder_success() {
        let runner = MockRunner;
        let result = runner
            .command("echo")
            .args(["Hello", "world!"])
            .run()
            .expect("Command should succeed");

        assert_eq!(result.stdout, "Hello world!");
        assert!(result.success);
    }

    #[test]
    fn test_command_builder_failure() {
        let runner = MockRunner;
        let result = runner
            .command("fail")
            .run()
            .unwrap()
            .expect_success("This should return an error");

        assert!(result.is_err());
        let error = format!("{}", result.unwrap_err());
        assert!(error.contains("Something went wrong!"));
    }

    #[test]
    fn test_command_invalid() {
        let runner = MockRunner;
        let result = runner.command("invalid_command").run();

        assert!(result.is_err());
        let error = format!("{}", result.unwrap_err());
        assert!(error.contains("Unknown command"));
    }

    #[test]
    fn test_command_result_expect_success_with_stdout() {
        let result = CommandResult {
            stdout: "All good!".to_string(),
            stderr: String::new(),
            success: true,
        };

        let stdout = result.expect_success_with_stdout("mock_cmd").unwrap();
        assert_eq!(stdout, "All good!");
    }

    #[test]
    fn test_command_result_expect_success_failure() {
        let result = CommandResult {
            stdout: String::new(),
            stderr: "Error!".to_string(),
            success: false,
        };

        let err = result.expect_success("Command failed").unwrap_err();
        assert_eq!(format!("{}", err), "Command failed: Error!");
    }
}
