use std::env;
use std::fs;
use std::fs::File;
#[allow(unused_imports)]
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const SHELL_BUILTINS: &[&str] = &["exit", "echo", "type", "pwd", "cd"];

fn is_executable(path: &std::path::Path) -> bool {
    if let Ok(metadata) = fs::metadata(path) {
        return metadata.permissions().mode() & 0o111 != 0;
    }

    false
}

fn find_in_path(command: &str) -> Option<String> {
    let Some(path_os) = env::var_os("PATH") else {
        return None;
    };

    for dir in env::split_paths(&path_os) {
        let candidate = dir.join(command);
        if candidate.exists() && is_executable(&candidate) {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

/// Replaces the manual char loop and .split(' ')
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut inside_single_quote = false;
    let mut inside_double_quote = false;

    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\'' if !inside_double_quote => {
                inside_single_quote = !inside_single_quote;
                // Note: We don't push the quote itself to the token
            }
            '"' if !inside_single_quote => {
                inside_double_quote = !inside_double_quote;
            }
            '\\' if !inside_single_quote => {
                if let Some(&next_c) = chars.peek() {
                    if inside_double_quote {
                        // Inside double quotes, only specific chars are escaped
                        if next_c == '\\' || next_c == '"' || next_c == '$' || next_c == '\n' {
                            current.push(chars.next().unwrap());
                        } else {
                            current.push('\\');
                        }
                    } else {
                        // Outside quotes, backslash escapes the very next char
                        current.push(chars.next().unwrap());
                    }
                }
            }
            ' ' if !inside_single_quote && !inside_double_quote => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

struct CommandContext {
    argv: Vec<String>,
    stdout_file: Option<File>,
    stderr_file: Option<File>,
}

impl CommandContext {
    fn parse(tokens: Vec<String>) -> Self {
        let mut final_argv = Vec::new();
        let mut stdout_path = None;
        let mut stderr_path = None;
        let mut append_stdout = false;
        let mut append_stderr = false;

        let mut i = 0;
        while i < tokens.len() {
            match tokens[i].as_str() {
                ">" | "1>" => {
                    stdout_path = tokens.get(i + 1).cloned();
                    append_stdout = false;
                    i += 2;
                }
                ">>" | "1>>" => {
                    stdout_path = tokens.get(i + 1).cloned();
                    append_stdout = true;
                    i += 2;
                }
                "2>" => {
                    stderr_path = tokens.get(i + 1).cloned();
                    append_stderr = false;
                    i += 2;
                }
                "2>>" => {
                    stderr_path = tokens.get(i + 1).cloned();
                    append_stderr = true;
                    i += 2;
                }
                _ => {
                    final_argv.push(tokens[i].clone());
                    i += 1;
                }
            }
        }

        let open_file = |path: String, append: bool| {
            fs::OpenOptions::new()
                .write(true)
                .create(true)
                .append(append)
                .truncate(!append)
                .open(path)
                .ok()
        };

        Self {
            argv: final_argv,
            stdout_file: stdout_path.and_then(|p| open_file(p, append_stdout)),
            stderr_file: stderr_path.and_then(|p| open_file(p, append_stderr)),
        }
    }
}

fn execute_command(input: &str) -> bool {
    let argv = tokenize(input);
    let ctx = CommandContext::parse(argv);

    let command = &ctx.argv[0];
    let args = &ctx.argv[1..];

    match command.as_str() {
        "exit" => {
            return false;
        }
        "echo" => {
            let output = args.join(" ");
            if let Some(mut file) = ctx.stdout_file {
                writeln!(file, "{}", output).unwrap();
            } else {
                println!("{}", output);
            }
        }
        "type" => {
            let Some(query) = args.get(0) else {
                return true;
            };

            let res = if SHELL_BUILTINS.contains(&query.as_str()) {
                format!("{} is a shell builtin", query)
            } else if let Some(full_path) = find_in_path(query) {
                format!("{} is {}", query, full_path)
            } else {
                format!("{}: not found", query)
            };

            if let Some(mut file) = ctx.stdout_file {
                writeln!(file, "{}", res).unwrap();
            } else {
                println!("{}", res);
            }
        }
        "pwd" => {
            println!("{}", env::current_dir().unwrap().display())
        }
        "cd" => {
            let home_dir = env::var("HOME").unwrap();
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
                println!("cd: {}: No such file or directory", display_path);
            }
        }
        _ => {
            if let Some(_path) = find_in_path(command) {
                let mut cmd = Command::new(command);
                cmd.args(args);

                if let Some(file) = ctx.stdout_file {
                    cmd.stdout(file);
                }
                if let Some(file) = ctx.stderr_file {
                    cmd.stderr(file);
                }

                cmd.status().unwrap();
            } else {
                println!("{}: not found", command);
            }
        }
    }
    true
}

fn main() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();

        if input.is_empty() {
            continue;
        }

        if !execute_command(input) {
            break;
        }
    }
}
