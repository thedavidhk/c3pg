use crate::command_runner::{CommandResult, CommandRunner, StreamMode};
use std::cell::RefCell;

struct MockResponse {
    cmd: String,
    args_contain: Vec<String>,
    result: CommandResult,
}

pub struct MockCommandRunner {
    responses: RefCell<Vec<MockResponse>>,
    executed_commands: RefCell<Vec<(String, Vec<String>)>>,
    default_result: CommandResult,
}

impl MockCommandRunner {
    /// Create a mock runner where unmatched commands return the given output.
    /// Pass `None` to simulate command failure by default.
    #[must_use] 
    pub fn new(output: Option<String>) -> Self {
        let success = output.is_some();
        let stdout = output.unwrap_or_default();
        Self {
            responses: RefCell::new(vec![]),
            executed_commands: RefCell::new(vec![]),
            default_result: CommandResult {
                stdout,
                stderr: if success {
                    String::new()
                } else {
                    "Error".to_string()
                },
                success,
            },
        }
    }

    /// Register a response for a specific command. When `execute` is called,
    /// responses are checked in registration order. The first response whose
    /// `cmd` matches and whose `args_contain` entries all appear in the actual
    /// args is used. If nothing matches, `default_result` is returned.
    pub fn on(&self, cmd: &str, args_contain: &[&str], result: CommandResult) -> &Self {
        self.responses.borrow_mut().push(MockResponse {
            cmd: cmd.to_string(),
            args_contain: args_contain.iter().map(std::string::ToString::to_string).collect(),
            result,
        });
        self
    }

    /// Convenience: register a successful response with the given stdout.
    pub fn on_success(&self, cmd: &str, args_contain: &[&str], stdout: &str) -> &Self {
        self.on(
            cmd,
            args_contain,
            CommandResult {
                stdout: stdout.to_string(),
                stderr: String::new(),
                success: true,
            },
        )
    }

    /// Return a clone of all executed (command, args) pairs in order.
    pub fn executed_commands(&self) -> Vec<(String, Vec<String>)> {
        self.executed_commands.borrow().clone()
    }

    /// Assert that a command matching `cmd` with all `args_contain` present was executed.
    pub fn assert_ran(&self, cmd: &str, args_contain: &[&str]) {
        let cmds = self.executed_commands.borrow();
        let found = cmds.iter().any(|(c, a)| {
            c == cmd && args_contain.iter().all(|needle| a.iter().any(|arg| arg == needle))
        });
        assert!(
            found,
            "Expected command `{cmd}` with args containing {args_contain:?} to have been run.\n\
             Actual commands: {cmds:?}"
        );
    }

    /// Assert that no command with the given name was executed.
    pub fn assert_did_not_run(&self, cmd: &str) {
        let cmds = self.executed_commands.borrow();
        let found = cmds.iter().any(|(c, _)| c == cmd);
        assert!(
            !found,
            "Expected command `{cmd}` NOT to have been run, but it was.\n\
             Actual commands: {cmds:?}"
        );
    }
}

impl Default for MockCommandRunner {
    fn default() -> Self {
        Self::new(Some(String::new()))
    }
}

impl CommandRunner for MockCommandRunner {
    fn execute(
        &self,
        cmd: &str,
        args: &[&str],
        _mode: StreamMode,
        _env: &[(&str, &str)],
    ) -> anyhow::Result<CommandResult> {
        self.executed_commands.borrow_mut().push((
            cmd.to_string(),
            args.iter().map(|&s| s.to_string()).collect(),
        ));

        // Find first matching response
        let responses = self.responses.borrow();
        for resp in responses.iter() {
            if resp.cmd == cmd
                && resp
                    .args_contain
                    .iter()
                    .all(|needle| args.contains(&needle.as_str()))
            {
                return Ok(resp.result.clone());
            }
        }

        // Fall back to default
        Ok(self.default_result.clone())
    }
}
