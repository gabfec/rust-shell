#[allow(unused_imports)]
use std::io::{self, Write};
use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
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
        let argv: Vec<&str> = command.trim().split(' ').collect();
        let args = &argv[1..];
        match argv[0] {
            "exit" => break,
            "echo" => println!("{}", args.join(" ") ),
            "type" => {
                let Some(query) = args.get(0).copied() else {
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
            "pwd" => {println!("{}", env::current_dir().unwrap().display())},
            "cd" => {
                let home_dir = env::var("HOME").unwrap();
                let path = match args.get(0).copied() {
                    None => Path::new(&home_dir).to_path_buf(),
                    Some(raw_arg) => {
                        if let Some(rest) = raw_arg.strip_prefix('~') {
                            Path::new(&home_dir).join(rest)
                        } else {
                            Path::new(raw_arg).to_path_buf()
                        }
                    }
                };

                if let Err(_) = env::set_current_dir(&path) {
                    let display_path = args.get(0).copied().unwrap_or("~");
                    println!("cd: {}: {}", display_path, "No such file or directory");
                }
            }
            _ =>  match find_in_path(argv[0]) {
                    Some(_) => {
                        Command::new(argv[0])
                            .args(args)
                            .status().unwrap();
                    },
                    None => { println!("{}: not found", argv[0])}
                }
        }
    }
}
