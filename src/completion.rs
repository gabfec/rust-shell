use crate::builtins::SHELL_BUILTINS;
use crate::utils::is_executable;
use rustyline::completion::Completer;
use rustyline::{Context, Helper, Highlighter, Hinter, Validator};
use std::env;
use std::fs;

#[derive(Helper, Hinter, Highlighter, Validator)]
pub struct ShellHelper;

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
