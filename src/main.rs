use std::env;
use std::fs;
use std::fs::File;
use std::io::Read;
#[allow(unused_imports)]
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
            set_raw_mode(false);
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

fn execute_pipeline(input: &str) -> bool {
    // Check for pipes
    if !input.contains('|') {
        return execute_command(input);
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

fn set_raw_mode(enable: bool) {
    let state = if enable { "raw" } else { "-raw" };
    let echo = if enable { "-echo" } else { "echo" };
    Command::new("stty").arg(state).arg(echo).status().ok();
}

fn handle_autocomplete(buffer: &mut String, tab_count: u32) {
    let mut matches = Vec::new();

    // Check Builtins
    for builtin in SHELL_BUILTINS {
        if builtin.starts_with(buffer.as_str()) {
            matches.push(builtin.to_string());
        }
    }

    // Check PATH
    if let Some(path_var) = env::var_os("PATH") {
        for dir in env::split_paths(&path_var) {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with(buffer.as_str()) && is_executable(&entry.path()) {
                        if !matches.contains(&name) {
                            matches.push(name);
                        }
                    }
                }
            }
        }
    }

    matches.sort();

    match matches.len() {
        0 => {
            // No match: ring the bell
            print!("\x07");
            io::stdout().flush().unwrap();
        }
        1 => {
            // Single match: complete it
            let completion = &matches[0][buffer.len()..];
            print!("{} ", completion);
            buffer.push_str(completion);
            buffer.push(' ');
            io::stdout().flush().unwrap();
        }
        _ => {
            // Multiple matches logic
            handle_multiple_matches(buffer, matches, tab_count);
        }
    }
}

fn handle_multiple_matches(buffer: &mut String, matches: Vec<String>, tab_count: u32) {
    if tab_count == 1 {
        // Longest Common Prefix (LCP) Logic
        let first = &matches[0];
        let mut lcp_len = buffer.len();

        'outer: for i in buffer.len()..first.len() {
            let char_at_i = first.chars().nth(i).unwrap();
            for m in &matches {
                if m.chars().nth(i) != Some(char_at_i) {
                    break 'outer;
                }
            }
            lcp_len += 1;
        }

        if lcp_len > buffer.len() {
            let extra = &first[buffer.len()..lcp_len];
            print!("{}", extra);
            buffer.push_str(extra);
        } else {
            print!("\x07"); // Bell if no more common chars
        }
    } else if tab_count >= 2 {
        // Double Tab Listing Logic
        println!(); // New line for the list
        println!("\r{}\r", matches.join("  "));
        print!("$ {}", buffer); // Restore the prompt line
    }
    let _ = io::stdout().flush();
}

fn main() {
    let mut input_buffer = String::new();
    let mut tab_count = 0;

    loop {
        print!("$ ");
        io::stdout().flush().unwrap();
        input_buffer.clear();

        // Switch to raw mode to intercept Tab
        set_raw_mode(true);

        loop {
            let mut buffer = [0; 1];
            io::stdin().read_exact(&mut buffer).unwrap();
            let c = buffer[0] as char;

            if c != '\t' {
                tab_count = 0;
            }

            match c {
                '\r' | '\n' => {
                    // Enter key pressed
                    set_raw_mode(false); // Back to normal to print output
                    println!();
                    if !input_buffer.is_empty() {
                        if !execute_pipeline(input_buffer.trim()) {
                            std::process::exit(0);
                        }
                    }
                    break; // Exit inner loop to show new prompt
                }
                '\t' => {
                    // TAB logic
                    tab_count += 1;
                    handle_autocomplete(&mut input_buffer, tab_count);
                }
                '\x7f' => {
                    // Backspace logic
                    if !input_buffer.is_empty() {
                        input_buffer.pop();
                        print!("\x08 \x08"); // Move back, overwrite with space, move back
                        io::stdout().flush().unwrap();
                    }
                }
                '\x03' => {
                    // Ctrl+C
                    set_raw_mode(false);
                    std::process::exit(0);
                }
                _ => {
                    // Normal character
                    input_buffer.push(c);
                    print!("{}", c);
                    io::stdout().flush().unwrap();
                }
            }
        }
    }
}
