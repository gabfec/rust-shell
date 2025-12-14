#[allow(unused_imports)]
use std::io::{self, Write};
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;

const SHELL_BUILTINS: &[&str] = &["exit", "echo", "type"];

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

        // If the file exists but lacks execute permissions, skip it and continue.
        if candidate.exists() && !is_executable(&candidate) {
            continue;
        }

        if is_executable(&candidate) {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }

    None
}

fn main() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        // Wait for user input
        let mut command = String::new();
        io::stdin().read_line(&mut command).unwrap();
        let cmd = command.trim().split(' ').collect::<Vec<&str>>();
        let args = cmd[1..].to_vec();
        match cmd[0] {
            "exit" => break,
            "echo" => println!("{}", args.join(" ") ),
            "type" => {
                let Some(query) = cmd.get(1).copied() else {
                    continue;
                };

                if SHELL_BUILTINS.contains(&query)  {
                    println!("{} is a shell builtin", &query);
                } else if let Some(full_path) = find_in_path(query) {
                    println!("{} is {}", query, full_path);
                } else {
                    println!("{}: not found", query);
                }
            },
            _ => println!("{}: command not found", command.trim()),
        }
    }
}
