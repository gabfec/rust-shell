use std::fs;
use std::fs::File;

pub fn tokenize(input: &str) -> Vec<String> {
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

pub struct CommandContext {
    pub argv: Vec<String>,
    pub stdout_file: Option<File>,
    pub stderr_file: Option<File>,
}

impl CommandContext {
    pub fn parse(tokens: Vec<String>) -> Self {
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
