use crate::builtins::{is_builtin, run_builtin};
use crate::parser::{SimpleCommand, Target, parse};
use crate::shell::{Flow, Shell};
use crate::utils::find_in_path;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::process::{Child, Command, Stdio};
use std::thread;

/// Runs one command line: a single command or `A | B | ... | N`,
/// optionally followed by `&` to run it in the background.
pub fn run_pipeline(shell: &mut Shell, line: &str) -> Flow {
    let pipeline = parse(line, &|name| shell.variable(name));
    // Every redirection is opened (creating its file) before anything runs.
    // A stage whose redirection fails is reported and left out (`None`).
    let stages: Vec<Option<Stage>> = pipeline.stages.into_iter().map(Stage::open).collect();
    if stages.iter().flatten().any(|stage| stage.argv.is_empty()) {
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
        let Some(stage) = stage else {
            continue;
        };

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
    if pipeline.background {
        let pid = children.last().unwrap().id();
        let id = shell.jobs.add(pipeline.text, children);
        println!("[{}] {}", id, pid);
    } else {
        for mut child in children {
            let _ = child.wait();
        }
    }
    Flow::Continue
}

/// A pipeline stage ready to run, with its redirections opened.
struct Stage {
    argv: Vec<String>,
    stdout: Option<File>,
    stderr: Option<File>,
}

impl Stage {
    /// Opens the command's redirections. If one can't be opened, reports
    /// it like bash (`<path>: <reason>`) and returns `None`: the command
    /// must not run.
    fn open(command: SimpleCommand) -> Option<Self> {
        let open = |target: &Option<Target>| match target {
            None => Ok(None),
            Some(target) => open_target(target).map(Some).map_err(|e| {
                eprintln!("{}: {}", target.path, describe(&e));
            }),
        };
        Some(Stage {
            stdout: open(&command.stdout).ok()?,
            stderr: open(&command.stderr).ok()?,
            argv: command.argv,
        })
    }
}

/// An I/O error's message without the "(os error N)" suffix.
fn describe(error: &io::Error) -> String {
    let message = error.to_string();
    match message.find(" (os error") {
        Some(end) => message[..end].to_string(),
        None => message,
    }
}

/// Opens a redirection target for writing, truncating it unless appending.
fn open_target(target: &Target) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .append(target.append)
        .truncate(!target.append)
        .open(&target.path)
}

/// Runs a builtin stage. Returns its flow and, for a stage in the middle of
/// a pipeline, a pipe carrying its output.
fn run_builtin_stage(shell: &mut Shell, stage: Stage, is_last: bool) -> (Flow, Option<Stdio>) {
    let Stage {
        argv,
        stdout: stdout_file,
        stderr: stderr_file,
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

fn spawn_external(stage: Stage, stdin: Option<Stdio>, is_last: bool) -> Option<Child> {
    let Stage {
        argv,
        stdout: stdout_file,
        stderr: stderr_file,
    } = stage;
    let name = &argv[0];
    if name.is_empty() || find_in_path(name).is_none() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_dir;
    use std::fs;

    /// Writes "new" through a redirection to a file that held "old".
    fn write_through(name: &str, append: bool) -> String {
        let dir = temp_dir(name);
        let path = dir.join("f");
        fs::write(&path, "old\n").unwrap();
        let target = Target {
            path: path.display().to_string(),
            append,
        };
        writeln!(open_target(&target).unwrap(), "new").unwrap();
        fs::read_to_string(&path).unwrap()
    }

    #[test]
    fn single_redirect_truncates() {
        assert_eq!(write_through("redirect-truncate", false), "new\n");
    }

    #[test]
    fn double_redirect_appends() {
        assert_eq!(write_through("redirect-append", true), "old\nnew\n");
    }

    #[test]
    fn os_error_suffix_is_dropped() {
        let error = fs::File::open("/no/such/file").unwrap_err();
        assert_eq!(describe(&error), "No such file or directory");
        assert_eq!(describe(&io::Error::other("custom")), "custom");
    }

    #[test]
    fn redirect_creates_the_file() {
        let dir = temp_dir("redirect-create");
        let target = Target {
            path: dir.join("new").display().to_string(),
            append: true,
        };
        open_target(&target).unwrap();
        assert!(dir.join("new").exists());
    }
}
