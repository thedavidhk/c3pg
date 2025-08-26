use crate::command_runner::{CommandResult, CommandRunner, StreamMode};
use std::cell::RefCell;

pub struct MockCommandRunner {
    executed_commands: RefCell<Vec<(String, Vec<String>)>>,
    output: RefCell<Option<String>>,
}

impl MockCommandRunner {
    pub fn new(output: Option<String>) -> Self {
        Self {
            executed_commands: RefCell::new(vec![]),
            output: RefCell::new(output),
        }
    }

    pub fn executed_commands(&self) -> Vec<(String, Vec<String>)> {
        self.executed_commands.borrow().clone()
    }
}

impl Default for MockCommandRunner {
    fn default() -> Self {
        Self {
            executed_commands: Default::default(),
            output: RefCell::new(Some(String::new())),
        }
    }
}

impl CommandRunner for MockCommandRunner {
    fn execute(&self, cmd: &str, args: &[&str], _mode: StreamMode) -> anyhow::Result<CommandResult> {
        self.executed_commands.borrow_mut().push((
            cmd.to_string(),
            args.iter().map(|&s| s.to_string()).collect(),
        ));

        let success = self.output.borrow().is_some();
        let stdout = self.output.borrow().clone().unwrap_or_default();

        Ok(CommandResult {
            stdout,
            stderr: if success {
                String::new()
            } else {
                "Error".to_string()
            },
            success,
        })
    }
}
