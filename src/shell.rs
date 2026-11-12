use crate::executor::run_pipeline;
use crate::history::History;
use crate::jobs::{JobTable, print_jobs};
use std::collections::BTreeMap;
use std::{env, io};

/// Whether the REPL should keep going after a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Exit,
}

/// All state that lives across command lines.
#[derive(Default)]
pub struct Shell {
    pub history: History,
    pub jobs: JobTable,
    /// Completer scripts registered with `complete -C`, by command name.
    pub completions: BTreeMap<String, String>,
    /// Shell variables set with `declare`.
    pub variables: BTreeMap<String, String>,
}

impl Shell {
    /// Records `line` in the history and runs it.
    pub fn run(&mut self, line: &str) -> Flow {
        self.history.push(line);
        run_pipeline(self, line)
    }

    /// The value of a shell variable, falling back to the environment.
    pub fn variable(&self, name: &str) -> Option<String> {
        self.variables
            .get(name)
            .cloned()
            .or_else(|| env::var(name).ok())
    }

    /// Prints (and forgets) background jobs that finished since last time.
    pub fn report_finished_jobs(&mut self) {
        let statuses = self.jobs.reap();
        let _ = print_jobs(&statuses, false, &mut io::stdout());
    }
}
