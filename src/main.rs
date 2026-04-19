mod builtins;
mod executor;
mod parser;
mod utils;

use crate::builtins::SHELL_BUILTINS;
use crate::executor::execute_pipeline;
use crate::utils::is_executable;
use rustyline::Helper;
use rustyline::completion::Completer;
use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Context, Editor, Highlighter, Hinter, Validator};
use std::env;
use std::fs;
use std::io::Write;

#[derive(Helper, Hinter, Highlighter, Validator)]
struct ShellHelper;

impl Completer for ShellHelper {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        let buffer = &line[..pos];

        // If there's a space, we're completing a filename argument
        if let Some(last_space) = buffer.rfind(' ') {
            let prefix = &buffer[last_space + 1..];
            let start = last_space + 1;

            let (dir, file_prefix, dir_prefix) = if let Some(slash) = prefix.rfind('/') {
                (&prefix[..slash + 1], &prefix[slash + 1..], &prefix[..slash + 1])
            } else {
                (".", prefix, "")
            };

            let mut matches = Vec::new();
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with(file_prefix) {
                        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        let suffix = if is_dir { "/" } else { " " };
                        matches.push(format!("{}{}{}", dir_prefix, name, suffix));
                    }
                }
            }

            matches.sort();

            return Ok((start, matches));
        }

        // No space: complete command (builtins + PATH)
        let mut matches = Vec::new();

        // Check Builtins
        for builtin in SHELL_BUILTINS {
            if builtin.starts_with(buffer) {
                matches.push(builtin.to_string());
            }
        }

        // Check PATH
        if let Some(path_var) = env::var_os("PATH") {
            for dir in env::split_paths(&path_var) {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name.starts_with(buffer) && is_executable(&entry.path()) {
                            if !matches.contains(&name) {
                                matches.push(name);
                            }
                        }
                    }
                }
            }
        }

        if matches.is_empty() {
            return Ok((0, Vec::new()));
        }

        matches.sort();
        matches.dedup();

        if matches.len() == 1 {
            // Single unique match -> Append space
            let mut completion = matches[0].clone();
            completion.push(' ');
            Ok((0, vec![completion]))
        } else {
            // Multiple matches -> Find Longest Common Prefix (LCP)
            // Let Rustyline handle the LCP and the Bell
            Ok((0, matches))
        }
    }
}

fn load_startup_history(history: &mut Vec<String>, rl: &mut Editor<ShellHelper, DefaultHistory>) {
    if let Ok(hist_path) = std::env::var("HISTFILE") {
        if let Ok(content) = std::fs::read_to_string(&hist_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    history.push(trimmed.to_string());
                    let _ = rl.add_history_entry(trimmed);
                }
            }
        }
    }
}

fn save_shutdown_history(history_vec: &[String]) {
    if let Ok(hist_path) = std::env::var("HISTFILE") {
        if let Ok(mut f) = std::fs::File::create(hist_path) {
            for entry in history_vec {
                let _ = writeln!(f, "{}", entry);
            }
        }
    }
}

fn main() -> rustyline::Result<()> {
    // Specify types explicitly to help the compiler infer 'line' type
    let mut rl: Editor<ShellHelper, DefaultHistory> = Editor::new()?;
    rl.set_helper(Some(ShellHelper));

    // It tells rustyline to only complete up to the common part.
    rl.set_completion_type(rustyline::CompletionType::List);

    let mut history: Vec<String> = Vec::new();

    // Startup load
    load_startup_history(&mut history, &mut rl);
    let mut last_sync_index = history.len(); // Tracks what has been written to disk

    loop {
        let readline = rl.readline("$ ");

        match readline {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Internal rustyline history (for UP arrow)
                let _ = rl.add_history_entry(trimmed);

                let command = trimmed.to_string();
                history.push(command); // Record the command

                if !execute_pipeline(trimmed, &mut history, &mut last_sync_index) {
                    break;
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C
                println!(); // Simply print a newline and continue the loop for a new prompt
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl-D
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    save_shutdown_history(&history);
    Ok(())
}
