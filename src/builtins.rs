use crate::jobs::print_jobs;
use crate::parser::CommandContext;
use crate::shell::{Flow, Shell};
use crate::utils::find_in_path;
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const SHELL_BUILTINS: &[&str] = &["exit", "echo", "type", "pwd", "cd", "history", "jobs"];

// Returns Some(flow) if the command was a builtin.
// Returns None if the command is not a builtin and should be handled externally.
pub fn handle_builtin(ctx: &CommandContext, shell: &mut Shell) -> Option<Flow> {
    let command = ctx.argv[0].as_str();
    let args = &ctx.argv[1..];

    match command {
        "exit" => Some(Flow::Exit),

        "echo" => {
            let output = args.join(" ");
            if let Some(mut file) = ctx.stdout_file.as_ref() {
                writeln!(file, "{}", output).ok();
            } else {
                println!("{}", output);
            }
            Some(Flow::Continue)
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
            Some(Flow::Continue)
        }

        "pwd" => {
            let res = env::current_dir().unwrap().display().to_string();
            if let Some(mut file) = ctx.stdout_file.as_ref() {
                writeln!(file, "{}", res).ok();
            } else {
                println!("{}", res);
            }
            Some(Flow::Continue)
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
            Some(Flow::Continue)
        }

        "history" => {
            handle_history_command(args, shell);
            Some(Flow::Continue)
        }

        "jobs" => {
            let _ = print_jobs(&shell.jobs.reap(), true, &mut io::stdout());
            Some(Flow::Continue)
        }

        _ => None,
    }
}

fn handle_history_command(args: &[String], shell: &mut Shell) {
    let history = &mut shell.history;
    let path = args.get(1);
    // File errors are silently ignored.
    match (args.first().map(String::as_str), path) {
        (Some("-r"), Some(path)) => drop(history.read_from(path)),
        (Some("-w"), Some(path)) => drop(history.write_to(path)),
        (Some("-a"), Some(path)) => drop(history.append_to(path)),
        (Some("-r" | "-w" | "-a"), None) => {}
        (arg, _) => {
            let count = arg.and_then(|s| s.parse().ok());
            let _ = history.print(count, &mut io::stdout());
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
