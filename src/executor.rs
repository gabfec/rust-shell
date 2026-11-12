use crate::builtins::{is_builtin, run_builtin};
use crate::parser::{CommandContext, parse_pipeline};
use crate::shell::{Flow, Shell};
use crate::utils::find_in_path;
use std::io::{self, Write};
use std::process::{Child, Command, Stdio};
use std::thread;

/// Runs one command line: a single command or `A | B | ... | N`,
/// optionally followed by `&` to run it in the background.
pub fn run_pipeline(shell: &mut Shell, line: &str) -> Flow {
    let mut stages = parse_pipeline(line, &|name| shell.variable(name));

    let background = stages
        .last()
        .and_then(|stage| stage.argv.last())
        .is_some_and(|arg| arg == "&");
    if background {
        stages.last_mut().unwrap().argv.pop();
    }
    if stages.iter().any(|stage| stage.argv.is_empty()) {
        return Flow::Continue;
    }

    let single = stages.len() == 1;
    let last = stages.len() - 1;
    let mut children = Vec::new();
    // Output of the previous stage, to become the next stage's stdin.
    let mut prev_stdout: Option<Stdio> = None;

    for (i, stage) in stages.into_iter().enumerate() {
        let is_last = i == last;
        // A stage after one that produced no pipe (redirected to a file,
        // or failed to start) reads nothing instead of the terminal.
        let stdin = (i > 0).then(|| prev_stdout.take().unwrap_or_else(Stdio::null));

        if is_builtin(&stage.argv[0]) {
            // Builtins don't read stdin.
            drop(stdin);
            let (flow, stdout) = run_builtin_stage(shell, stage, is_last);
            // `exit` only exits when it isn't part of a pipeline.
            if single && flow == Flow::Exit {
                return Flow::Exit;
            }
            prev_stdout = stdout;
        } else if let Some(mut child) = spawn_external(stage, stdin, is_last) {
            if !is_last {
                prev_stdout = child.stdout.take().map(Stdio::from);
            }
            children.push(child);
        }
    }

    if children.is_empty() {
        return Flow::Continue;
    }
    if background {
        let pid = children.last().unwrap().id();
        let command = line.trim_end().trim_end_matches('&').trim_end();
        let id = shell.jobs.add(command.to_string(), children);
        println!("[{}] {}", id, pid);
    } else {
        for mut child in children {
            let _ = child.wait();
        }
    }
    Flow::Continue
}

/// Runs a builtin stage. Returns its flow and, for a stage in the middle of
/// a pipeline, a pipe carrying its output.
fn run_builtin_stage(
    shell: &mut Shell,
    stage: CommandContext,
    is_last: bool,
) -> (Flow, Option<Stdio>) {
    let CommandContext {
        argv,
        stdout_file,
        stderr_file,
    } = stage;
    let mut err: Box<dyn Write> = match stderr_file {
        Some(file) => Box::new(file),
        None => Box::new(io::stderr()),
    };

    if let Some(mut file) = stdout_file {
        return (run_builtin(shell, &argv, &mut file, &mut err), None);
    }
    if is_last {
        return (run_builtin(shell, &argv, &mut io::stdout(), &mut err), None);
    }

    // Buffer the output first: the next stage isn't running yet, so writing
    // straight into a pipe could block once the pipe buffer fills up.
    let mut buffer = Vec::new();
    let flow = run_builtin(shell, &argv, &mut buffer, &mut err);
    (flow, pipe_from_bytes(buffer))
}

/// Returns a pipe whose read end yields `bytes` and then EOF.
fn pipe_from_bytes(bytes: Vec<u8>) -> Option<Stdio> {
    let (reader, mut writer) = io::pipe().ok()?;
    thread::spawn(move || {
        // Fails harmlessly if the reader exits early.
        let _ = writer.write_all(&bytes);
    });
    Some(reader.into())
}

fn spawn_external(stage: CommandContext, stdin: Option<Stdio>, is_last: bool) -> Option<Child> {
    let CommandContext {
        argv,
        stdout_file,
        stderr_file,
    } = stage;
    let name = &argv[0];
    if find_in_path(name).is_none() {
        eprintln!("{}: not found", name);
        return None;
    }

    let mut cmd = Command::new(name);
    cmd.args(&argv[1..]);
    if let Some(stdin) = stdin {
        cmd.stdin(stdin);
    }
    if let Some(file) = stdout_file {
        cmd.stdout(file);
    } else if !is_last {
        cmd.stdout(Stdio::piped());
    }
    if let Some(file) = stderr_file {
        cmd.stderr(file);
    }

    cmd.spawn().map_err(|e| eprintln!("{}: {}", name, e)).ok()
}
