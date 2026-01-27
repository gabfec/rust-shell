use std::env;
use std::fs;
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

        // Use the tokenizer instead of split(' ')
        let argv = tokenize(input);
        let command = &argv[0];
        let args = &argv[1..];

        match command.as_str() {
            "exit" => break,
            "echo" => println!("{}", args.join(" ")),
            "type" => {
                let Some(query) = args.get(0) else {
                    continue;
                };

                if SHELL_BUILTINS.contains(&query.as_str()) {
                    println!("{} is a shell builtin", query);
                } else if let Some(full_path) = find_in_path(query) {
                    println!("{} is {}", query, full_path);
                } else {
                    println!("{}: not found", query);
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
            _ => match find_in_path(command) {
                Some(_) => {
                    Command::new(command).args(args).status().unwrap();
                }
                None => {
                    println!("{}: not found", command)
                }
            },
        }
    }
}
