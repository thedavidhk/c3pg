use log::LevelFilter;

#[derive(Copy, Clone, Debug)]
pub enum StreamMode {
    /// Buffer stdout/stderr; no live output.
    Buffer,
    /// Stream selectively; still buffer everything for the result.
    Stream {
        stream_stdout: bool,
        stream_stderr: bool,
        /// If true, prefix live lines with `cmd:` (handy at -vv).
        prefix: bool,
    },
}

/// For external tools (cmake/conan):
/// -q / Error:    only show errors (stderr)
/// Info/Warn:     only show warnings/errors (stderr)
/// -v / -vv:      show stdout + stderr (prefix at -vv)
#[must_use] 
pub fn tool_stream_mode(level: LevelFilter) -> StreamMode {
    match level {
        LevelFilter::Off | LevelFilter::Error | LevelFilter::Warn | LevelFilter::Info => {
            StreamMode::Stream {
                stream_stdout: false,
                stream_stderr: true,
                prefix: false,
            }
        }
        LevelFilter::Debug => StreamMode::Stream {
            stream_stdout: true,
            stream_stderr: true,
            prefix: false,
        },
        LevelFilter::Trace => StreamMode::Stream {
            stream_stdout: true,
            stream_stderr: true,
            prefix: true,
        },
    }
}

/// For the user binary (`cmd_run`):
/// Default: show everything live.
/// -q / Error / Warn: suppress live output (buffer; show only on failure).
#[must_use] 
pub fn binary_stream_mode(level: LevelFilter) -> StreamMode {
    match level {
        LevelFilter::Off | LevelFilter::Error | LevelFilter::Warn => StreamMode::Buffer,
        _ => StreamMode::Stream {
            stream_stdout: true,
            stream_stderr: true,
            prefix: false, // keep it clean for app output
        },
    }
}

pub trait CommandRunner: Sized {
    /// Run `cmd` with the given arguments, streaming mode, and environment.
    ///
    /// # Errors
    ///
    /// Returns an error if the command cannot be spawned or an I/O error
    /// occurs while communicating with the child process.
    fn execute(
        &self,
        cmd: &str,
        args: &[&str],
        mode: StreamMode,
        env: &[(&str, &str)],
    ) -> anyhow::Result<CommandResult>;

    fn command(&self, cmd: impl Into<String>) -> CommandBuilder<&Self> {
        CommandBuilder::new(self, cmd)
    }
}

#[derive(Debug)]
pub struct CommandBuilder<R: CommandRunner> {
    runner: R,
    cmd: String,
    args: Vec<String>,
    mode: StreamMode,
    env: Vec<(String, String)>,
}

impl<R: CommandRunner> CommandBuilder<R> {
    pub fn new(runner: R, cmd: impl Into<String>) -> Self {
        Self {
            runner,
            cmd: cmd.into(),
            args: Vec::new(),
            mode: StreamMode::Buffer,
            env: Vec::new(),
        }
    }

    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    #[must_use]
    pub fn stream_mode(mut self, mode: StreamMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set an environment variable for the spawned process.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.env.push((key.into(), val.into()));
        self
    }

    /// Set multiple environment variables for the spawned process.
    #[must_use]
    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.env
            .extend(vars.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    /// Execute the built command and return the captured result.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying [`CommandRunner::execute`] call
    /// fails (e.g. the command cannot be found or spawned).
    pub fn run(self) -> anyhow::Result<CommandResult> {
        let args: Vec<_> = self.args.iter().map(String::as_str).collect();
        let env: Vec<(&str, &str)> = self
            .env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        self.runner.execute(&self.cmd, &args, self.mode, &env)
    }
}

#[derive()]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn execute(
        &self,
        cmd: &str,
        args: &[&str],
        mode: StreamMode,
        env: &[(&str, &str)],
    ) -> anyhow::Result<CommandResult> {
        use std::io::{BufRead, BufReader, Write};
        use std::process::{Command, Stdio};
        use std::sync::{Arc, Mutex};
        use std::thread;

        fn spawn_reader<R: std::io::Read + Send + 'static>(
            pipe: Option<R>,
            is_stdout: bool,
            buf: Arc<Mutex<String>>,
            stream: bool,
            cmd_prefix: Option<String>,
        ) -> thread::JoinHandle<()> {
            thread::spawn(move || {
                if let Some(pipe) = pipe {
                    let reader = BufReader::new(pipe);
                    for line in reader.lines().map_while(Result::ok) {
                        {
                            let mut b = buf.lock().unwrap();
                            b.push_str(&line);
                            b.push('\n');
                        }

                        if stream {
                            if is_stdout {
                                if let Some(ref p) = cmd_prefix {
                                    print!("{p}");
                                }
                                println!("{line}");
                                let _ = std::io::stdout().flush();
                            } else {
                                if let Some(ref p) = cmd_prefix {
                                    eprint!("{p}");
                                }
                                eprintln!("{line}");
                                let _ = std::io::stderr().flush();
                            }
                        }
                    }
                }
            })
        }

        let mut command = Command::new(cmd);
        command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in env {
            command.env(k, v);
        }
        let mut child = command.spawn()?;

        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        let out_buf = Arc::new(Mutex::new(String::new()));
        let err_buf = Arc::new(Mutex::new(String::new()));

        let (stream_stdout, stream_stderr, prefix) = match mode {
            StreamMode::Buffer => (false, false, false),
            StreamMode::Stream {
                stream_stdout,
                stream_stderr,
                prefix,
            } => (stream_stdout, stream_stderr, prefix),
        };

        let cmd_prefix = if prefix {
            Some(format!("{cmd}: "))
        } else {
            None
        };

        let t_out = spawn_reader(
            stdout_pipe,
            true,
            out_buf.clone(),
            stream_stdout,
            cmd_prefix.clone(),
        );
        let t_err = spawn_reader(
            stderr_pipe,
            false,
            err_buf.clone(),
            stream_stderr,
            cmd_prefix,
        );

        let status = child.wait()?;
        let _ = t_out.join();
        let _ = t_err.join();

        let stdout = Arc::try_unwrap(out_buf).unwrap().into_inner().unwrap();
        let stderr = Arc::try_unwrap(err_buf).unwrap().into_inner().unwrap();

        Ok(CommandResult {
            stdout,
            stderr,
            success: status.success(),
        })
    }
}

impl<T: CommandRunner + Sized> CommandRunner for &T {
    fn execute(
        &self,
        cmd: &str,
        args: &[&str],
        mode: StreamMode,
        env: &[(&str, &str)],
    ) -> anyhow::Result<CommandResult> {
        (*self).execute(cmd, args, mode, env)
    }
}

#[derive(Debug, Clone, Default)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

impl CommandResult {
    /// Returns the `stdout` if the command succeeded, or an error otherwise.
    ///
    /// # Errors
    ///
    /// Returns an error containing `cmd` and `stderr` when the command exited
    /// with a non-zero status.
    pub fn expect_success_with_stdout(self, cmd: &str) -> anyhow::Result<String> {
        if self.success {
            Ok(self.stdout)
        } else {
            anyhow::bail!("Command `{}` failed: {}", cmd, self.stderr);
        }
    }

    /// Ensures the command succeeded. Otherwise, returns an error.
    ///
    /// # Errors
    ///
    /// Returns an error prefixed with `error_msg` and including `stderr`
    /// when the command exited with a non-zero status.
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
    use std::cell::RefCell;

    #[derive(Default)]
    #[allow(clippy::struct_field_names)]
    struct MockRunner {
        // Capture what was invoked
        recorded_cmd: RefCell<Option<String>>,
        recorded_args: RefCell<Vec<String>>,
        recorded_mode: RefCell<Option<StreamMode>>,
    }

    impl CommandRunner for MockRunner {
        fn execute(
            &self,
            cmd: &str,
            args: &[&str],
            mode: StreamMode,
            _env: &[(&str, &str)],
        ) -> anyhow::Result<CommandResult> {
            // Record invocation for the verbosity/mode tests
            *self.recorded_cmd.borrow_mut() = Some(cmd.to_string());
            *self.recorded_args.borrow_mut() =
                args.iter().map(std::string::ToString::to_string).collect();
            *self.recorded_mode.borrow_mut() = Some(mode);

            // Simulate real command behavior expected by the tests
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
                // success stubs for verbosity mapping tests
                "conan" => Ok(CommandResult {
                    stdout: format!("conan {}", args.join(" ")),
                    stderr: String::new(),
                    success: true,
                }),
                "cmake" => {
                    let is_build = args.contains(&"--build");
                    let stdout = if is_build {
                        "Build finished".to_string()
                    } else {
                        "-- Configuring done\n-- Generating done\n-- Build files have been written to: /tmp/mock".to_string()
                    };
                    Ok(CommandResult {
                        stdout,
                        stderr: String::new(),
                        success: true,
                    })
                }
                _ => anyhow::bail!("Unknown command"),
            }
        }

        fn command(&self, cmd: impl Into<String>) -> CommandBuilder<&Self> {
            CommandBuilder::new(self, cmd)
        }
    }

    #[test]
    fn test_command_builder_success() {
        let runner = MockRunner::default();
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
        let runner = MockRunner::default();
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
        let runner = MockRunner::default();
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
        assert_eq!(format!("{err}"), "Command failed: Error!");
    }
}
