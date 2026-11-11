mod builtins;
mod completion;
mod executor;
mod history;
mod jobs;
mod parser;
mod shell;
#[cfg(test)]
mod test_util;
mod utils;

use crate::completion::ShellHelper;
use crate::shell::{Flow, Shell};
use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{CompletionType, Editor};
use std::env;

fn main() -> rustyline::Result<()> {
    let mut rl: Editor<ShellHelper, DefaultHistory> = Editor::new()?;
    rl.set_helper(Some(ShellHelper));
    // Only complete up to the common prefix, then list the candidates.
    rl.set_completion_type(CompletionType::List);

    let mut shell = Shell::default();
    let histfile = env::var("HISTFILE").ok();
    if let Some(path) = &histfile {
        let _ = shell.history.read_from(path);
        shell.history.mark_synced();
    }
    // How many history entries rustyline (UP arrow) has been given so far.
    let mut editor_history_len = 0;

    loop {
        // Covers both typed commands and entries loaded by `history -r`.
        for entry in &shell.history.entries()[editor_history_len..] {
            let _ = rl.add_history_entry(entry.as_str());
        }
        editor_history_len = shell.history.entries().len();

        shell.report_finished_jobs();

        match rl.readline("$ ") {
            Ok(line) => {
                let line = line.trim();
                if !line.is_empty() && shell.run(line) == Flow::Exit {
                    break;
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C: abandon the line and show a fresh prompt.
                println!();
            }
            Err(ReadlineError::Eof) => break, // Ctrl-D
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    if let Some(path) = &histfile {
        let _ = shell.history.write_to(path);
    }
    Ok(())
}
