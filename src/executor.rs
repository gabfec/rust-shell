use crate::builtins::{SHELL_BUILTINS, handle_builtin, run_builtin_capture};
use crate::parser::{CommandContext, tokenize};
use crate::shell::{Flow, Shell};
use crate::utils::find_in_path;
use std::process::{Command, Stdio};

// Helper to turn a String into a Stdio source (for builtins in the middle of pipes)
fn string_to_stdio(input: String) -> Stdio {
    let mut child = Command::new("printf")
        .arg(input)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    Stdio::from(child.stdout.take().unwrap())
}

fn execute_command(input: &str, shell: &mut Shell) -> Flow {
    let mut argv = tokenize(input);
    if argv.is_empty() {
        return Flow::Continue;
    }

    let background = argv.last().map(|s| s == "&").unwrap_or(false);
    if background {
        argv.pop();
    }

    let command_str = argv.join(" ");
    let ctx = CommandContext::parse(argv);

    // Try to run as a builtin first
    if let Some(flow) = handle_builtin(&ctx, shell) {
        return flow;
    }

    // Otherwise, look for an external executable
    let command = &ctx.argv[0];
    if let Some(_path) = find_in_path(command) {
        let mut cmd = Command::new(command);
        cmd.args(&ctx.argv[1..]);

        if let Some(file) = ctx.stdout_file {
            cmd.stdout(file);
        }
        if let Some(file) = ctx.stderr_file {
            cmd.stderr(file);
        }

        if background {
            match cmd.spawn() {
                Ok(child) => {
                    let pid = child.id();
                    let job_id = shell.jobs.add(command_str, vec![child]);
                    println!("[{}] {}", job_id, pid);
                }
                Err(e) => eprintln!("{}: {}", command, e),
            }
        } else {
            match cmd.status() {
                Ok(_) => {}
                Err(e) => eprintln!("{}: {}", command, e),
            }
        }
    } else {
        eprintln!("{}: not found", command);
    }

    Flow::Continue
}

pub fn execute_pipeline(shell: &mut Shell, input: &str) -> Flow {
    // Check for pipes
    if !input.contains('|') {
        return execute_command(input, shell);
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
    Flow::Continue
}
