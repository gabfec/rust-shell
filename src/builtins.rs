use crate::executor::Job;
use crate::parser::CommandContext;
use crate::utils::find_in_path;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SHELL_BUILTINS: &[&str] = &["exit", "echo", "type", "pwd", "cd", "history", "jobs"];

// Returns Some(bool) if the command was a builtin (bool is whether to continue the loop).
// Returns None if the command is not a builtin and should be handled externally.
pub fn handle_builtin(
    ctx: &CommandContext,
    history: &mut Vec<String>,
    last_sync_index: &mut usize,
    jobs: &mut Vec<Job>,
) -> Option<bool> {
    let command = ctx.argv[0].as_str();
    let args = &ctx.argv[1..];

    match command {
        "exit" => Some(false),

        "echo" => {
            let output = args.join(" ");
            if let Some(mut file) = ctx.stdout_file.as_ref() {
                writeln!(file, "{}", output).ok();
            } else {
                println!("{}", output);
            }
            Some(true)
        }

        "type" => {
            let res = if let Some(query) = args.get(0) {
                if SHELL_BUILTINS.contains(&query.as_str()) {
                    format!("{} is a shell builtin", query)
                } else if let Some(full_path) = find_in_path(query) {
                    format!("{} is {}", query, full_path)
                } else {
                    format!("{}: not found", query)
                }
            } else {
                String::new()
            };

            if let Some(mut file) = ctx.stdout_file.as_ref() {
                writeln!(file, "{}", res).ok();
            } else {
                println!("{}", res);
            }
            Some(true)
        }

        "pwd" => {
            let res = env::current_dir().unwrap().display().to_string();
            if let Some(mut file) = ctx.stdout_file.as_ref() {
                writeln!(file, "{}", res).ok();
            } else {
                println!("{}", res);
            }
            Some(true)
        }

        "cd" => {
            let home_dir = env::var("HOME").unwrap_or_else(|_| "/".to_string());
            let path = match args.get(0) {
                None => PathBuf::from(&home_dir),
                Some(raw_arg) => {
                    if let Some(rest) = raw_arg.strip_prefix('~') {
                        Path::new(&home_dir).join(rest)
                    } else {
                        PathBuf::from(raw_arg)
                    }
                }
            };

            if let Err(_) = env::set_current_dir(&path) {
                let display_path = args.get(0).map(|s| s.as_str()).unwrap_or("~");
                eprintln!("cd: {}: No such file or directory", display_path);
            }
            Some(true)
        }

        "history" => {
            handle_history_command(args, history, last_sync_index);
            Some(true)
        }

        "jobs" => {
            let job_entries = reap_jobs(jobs);
            print_jobs(&job_entries, true);
            Some(true)
        }

        _ => None,
    }
}

fn handle_history_command(args: &[String], history: &mut Vec<String>, last_sync_index: &mut usize) {
    let mut args_iter = args.iter();
    match args_iter.next().map(|s| s.as_str()) {
        Some("-r") => {
            if let Some(path) = args_iter.next() {
                if let Ok(content) = fs::read_to_string(path) {
                    for line in content.lines() {
                        if !line.trim().is_empty() {
                            history.push(line.to_string());
                        }
                    }
                }
            }
        }
        Some("-w") => {
            if let Some(path) = args_iter.next() {
                if let Ok(mut file) = fs::File::create(path) {
                    for entry in history.iter() {
                        let _ = writeln!(file, "{}", entry);
                    }
                    *last_sync_index = history.len();
                }
            }
        }
        Some("-a") => {
            if let Some(path) = args_iter.next() {
                if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
                    for i in *last_sync_index..history.len() {
                        let _ = writeln!(file, "{}", history[i]);
                    }
                    *last_sync_index = history.len();
                }
            }
        }
        arg => {
            let count = arg.and_then(|s| s.parse::<usize>().ok());
            let start_index = match count {
                Some(n) if n < history.len() => history.len() - n,
                _ => 0,
            };
            for i in start_index..history.len() {
                println!("{:>5}  {}", i + 1, history[i]);
            }
        }
    }
}

pub struct JobEntry {
    id: usize,
    marker: char,
    command: String,
    done: bool,
}

/// Polls each job's exit status, removes done jobs, and returns entries with markers.
pub fn reap_jobs(jobs: &mut Vec<Job>) -> Vec<JobEntry> {
    let n = jobs.len();
    let mut done_ids = Vec::new();
    for job in jobs.iter_mut() {
        if let Ok(Some(_)) = job.child.try_wait() {
            done_ids.push(job.id);
        }
    }
    let entries = jobs.iter().enumerate().map(|(i, job)| JobEntry {
        id: job.id,
        marker: if i == n - 1 { '+' } else if i == n - 2 { '-' } else { ' ' },
        command: job.command.clone(),
        done: done_ids.contains(&job.id),
    }).collect();
    jobs.retain(|j| !done_ids.contains(&j.id));
    entries
}

pub fn print_jobs(entries: &[JobEntry], show_running: bool) {
    for e in entries {
        if e.done {
            println!("[{}]{}  {:<24}{}", e.id, e.marker, "Done", e.command);
        } else if show_running {
            println!("[{}]{}  {:<24}{} &", e.id, e.marker, "Running", e.command);
        }
    }
}


// Helper for pipeline capturing (Builtins inside pipes)
pub fn run_builtin_capture(ctx: &CommandContext) -> String {
    match ctx.argv[0].as_str() {
        "echo" => ctx.argv[1..].join(" ") + "\n",
        "pwd" => env::current_dir().unwrap().display().to_string() + "\n",
        "type" => {
            if let Some(query) = ctx.argv.get(1) {
                if SHELL_BUILTINS.contains(&query.as_str()) {
                    format!("{} is a shell builtin\n", query)
                } else if let Some(path) = find_in_path(query) {
                    format!("{} is {}\n", query, path)
                } else {
                    format!("{}: not found\n", query)
                }
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}
