use rustyline::Helper;
use rustyline::completion::Completer;
use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{Context, Editor, Highlighter, Hinter, Validator};
use std::env;
use std::fs;
use std::fs::File;
#[allow(unused_imports)]
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SHELL_BUILTINS: &[&str] = &["exit", "echo", "type", "pwd", "cd", "history"];

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

fn execute_command(input: &str, history: &mut Vec<String>, last_sync_index: &mut usize) -> bool {
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
        "history" => {
            let mut args_iter = args.iter();
            match args_iter.next().map(|s| s.as_str()) {
                Some("-r") => {
                    if let Some(path) = args_iter.next() {
                        if let Ok(file_content) = fs::read_to_string(path) {
                            for line in file_content.lines() {
                                if !line.trim().is_empty() {
                                    history.push(line.to_string());
                                }
                            }
                        }
                    }
                }
                Some("-w") => {
                    if let Some(path) = args_iter.next() {
                        // Open file: Create if not exists, truncate if it does
                        if let Ok(mut file) = fs::File::create(path) {
                            for entry in history {
                                // Write each command followed by a newline
                                let _ = writeln!(file, "{}", entry);
                            }
                        }
                    }
                }
                Some("-a") => {
                    if let Some(path) = args_iter.next() {
                        // Open file in Append mode
                        let file_result =
                            fs::OpenOptions::new().create(true).append(true).open(path);

                        if let Ok(mut file) = file_result {
                            // Only write the "new" commands
                            for i in *last_sync_index..history.len() {
                                let _ = writeln!(file, "{}", history[i]);
                            }
                            // Update the offset so the next -a starts from here
                            *last_sync_index = history.len();
                        }
                    }
                }
                _ => {
                    // Standard history <n> logic
                    let count = args.get(0).and_then(|s| s.parse::<usize>().ok());

                    // Determine where to start printing
                    let start_index = match count {
                        Some(n) if n < history.len() => history.len() - n,
                        _ => 0,
                    };

                    for i in start_index..history.len() {
                        // Formatting: index starts at 1
                        println!("{:>5}  {}", i + 1, history[i]);
                    }
                }
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

fn execute_pipeline(input: &str, history: &mut Vec<String>, last_sync_index: &mut usize) -> bool {
    // Check for pipes
    if !input.contains('|') {
        return execute_command(input, history, last_sync_index);
    }

    // Split into segments
    let segments: Vec<&str> = input.split('|').map(|s| s.trim()).collect();
    let mut prev_stdout: Option<Stdio> = None;
    let mut children = Vec::new();

    // For a multiple-pipe: A | B | ... | N
    for (i, segment) in segments.iter().enumerate() {
        let is_last = i == segments.len() - 1;
        let ctx = CommandContext::parse(tokenize(segment));

        if SHELL_BUILTINS.contains(&ctx.argv[0].as_str()) {
            let output = run_builtin_capture(&ctx);
            if is_last {
                print!("{}", output);
            } else {
                // Bridge builtin output to next command via a small helper
                prev_stdout = Some(string_to_stdio(output));
            }
        } else {
            let mut cmd = Command::new(&ctx.argv[0]);
            cmd.args(&ctx.argv[1..]);

            // Connect plumbing
            if let Some(prev) = prev_stdout.take() {
                cmd.stdin(prev);
            }
            if !is_last {
                cmd.stdout(Stdio::piped());
            }

            let mut child = cmd.spawn().expect("Failed to spawn");

            if !is_last {
                prev_stdout = child.stdout.take().map(Stdio::from);
            }
            children.push(child);
        }
    }

    // Wait for all external processes to finish
    for mut child in children {
        let _ = child.wait();
    }
    true
}

// Helper to turn a String into a Stdio source (for builtins in the middle of pipes)
fn string_to_stdio(input: String) -> Stdio {
    let mut child = Command::new("printf")
        .arg(input)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    Stdio::from(child.stdout.take().unwrap())
}

fn run_builtin_capture(ctx: &CommandContext) -> String {
    match ctx.argv[0].as_str() {
        "echo" => ctx.argv[1..].join(" ") + "\n",
        "pwd" => env::current_dir().unwrap().display().to_string() + "\n",
        "type" => {
            let query = &ctx.argv[1];
            if SHELL_BUILTINS.contains(&query.as_str()) {
                format!("{} is a shell builtin\n", query)
            } else if let Some(path) = find_in_path(query) {
                format!("{} is {}\n", query, path)
            } else {
                format!("{}: not found\n", query)
            }
        }
        _ => String::new(),
    }
}

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
        let mut matches = Vec::new();
        let buffer = &line[..pos];

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

fn main() -> rustyline::Result<()> {
    // Specify types explicitly to help the compiler infer 'line' type
    let mut rl: Editor<ShellHelper, DefaultHistory> = Editor::new()?;
    rl.set_helper(Some(ShellHelper));

    // It tells rustyline to only complete up to the common part.
    rl.set_completion_type(rustyline::CompletionType::List);

    let mut history: Vec<String> = Vec::new();
    let mut last_sync_index = 0; // Tracks what has been written to disk

    // Startup load
    if let Ok(hist_path) = std::env::var("HISTFILE") {
        if let Ok(content) = std::fs::read_to_string(&hist_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    history.push(trimmed.to_string());
                }
            }
        }
    }

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
                break;
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
    Ok(())
}
