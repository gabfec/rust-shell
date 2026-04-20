use crate::builtins::{SHELL_BUILTINS, handle_builtin, run_builtin_capture};
use crate::parser::{CommandContext, tokenize};
use crate::utils::find_in_path;
use std::process::{Child, Command, Stdio};

pub struct Job {
    pub id: usize,
    pub _pid: u32,
    pub command: String,
    pub child: Child,
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

fn execute_command(input: &str, history: &mut Vec<String>, sync_idx: &mut usize, jobs: &mut Vec<Job>) -> bool {
    let mut argv = tokenize(input);
    if argv.is_empty() {
        return true;
    }

    let background = argv.last().map(|s| s == "&").unwrap_or(false);
    if background {
        argv.pop();
    }

    let command_str = argv.join(" ");
    let ctx = CommandContext::parse(argv);

    // Try to run as a builtin first
    if let Some(should_continue) = handle_builtin(&ctx, history, sync_idx, jobs) {
        return should_continue;
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
                    // Calculate smallest available ID
                    let mut job_id = 1;
                    let mut current_ids: Vec<usize> = jobs.iter().map(|j| j.id).collect();
                    current_ids.sort();

                    for &id in &current_ids {
                        if id == job_id {
                            job_id += 1;
                        } else if id > job_id {
                            // Found a gap, use this one
                            break;
                        }
                    }

                    let pid = child.id();
                    println!("[{}] {}", job_id, pid);
                    jobs.push(Job { id: job_id, _pid: pid, command: command_str, child });
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

    true
}

pub fn execute_pipeline(
    input: &str,
    history: &mut Vec<String>,
    last_sync_index: &mut usize,
    jobs: &mut Vec<Job>,
) -> bool {
    // Check for pipes
    if !input.contains('|') {
        return execute_command(input, history, last_sync_index, jobs);
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
