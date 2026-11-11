use crate::executor::execute_pipeline;
use crate::history::History;
use crate::jobs::{JobTable, print_jobs};
use std::io;

/// Whether the REPL should keep going after a command.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Exit,
}

/// All state that lives across command lines.
#[derive(Default)]
pub struct Shell {
    pub history: History,
    pub jobs: JobTable,
}

impl Shell {
    /// Records `line` in the history and runs it.
    pub fn run(&mut self, line: &str) -> Flow {
        self.history.push(line);
        execute_pipeline(self, line)
    }

    /// Prints (and forgets) background jobs that finished since last time.
    pub fn report_finished_jobs(&mut self) {
        let statuses = self.jobs.reap();
        let _ = print_jobs(&statuses, false, &mut io::stdout());
    }
}
