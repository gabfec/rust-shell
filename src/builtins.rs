use crate::jobs::print_jobs;
use crate::shell::{Flow, Shell};
use crate::utils::find_in_path;
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const SHELL_BUILTINS: &[&str] = &["exit", "echo", "type", "pwd", "cd", "history", "jobs"];

pub fn is_builtin(name: &str) -> bool {
    SHELL_BUILTINS.contains(&name)
}

/// Runs the builtin named by `argv[0]`, writing its output to `out` and its
/// errors to `err`. The caller must check `is_builtin` first.
pub fn run_builtin(
    shell: &mut Shell,
    argv: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Flow {
    let args = &argv[1..];
    // Write errors (e.g. a closed pipe) are not worth reporting from a builtin.
    let _ = match argv[0].as_str() {
        "exit" => return Flow::Exit,
        "echo" => writeln!(out, "{}", args.join(" ")),
        "type" => type_builtin(args, out),
        "pwd" => pwd(out, err),
        "cd" => cd(args, err),
        "history" => history(shell, args, out),
        "jobs" => print_jobs(&shell.jobs.reap(), true, out),
        other => unreachable!("{other} is not a builtin"),
    };
    Flow::Continue
}

fn type_builtin(args: &[String], out: &mut dyn Write) -> io::Result<()> {
    let Some(query) = args.first() else {
        return writeln!(out);
    };
    if is_builtin(query) {
        writeln!(out, "{} is a shell builtin", query)
    } else if let Some(full_path) = find_in_path(query) {
        writeln!(out, "{} is {}", query, full_path)
    } else {
        writeln!(out, "{}: not found", query)
    }
}

fn pwd(out: &mut dyn Write, err: &mut dyn Write) -> io::Result<()> {
    match env::current_dir() {
        Ok(dir) => writeln!(out, "{}", dir.display()),
        Err(e) => writeln!(err, "pwd: {}", e),
    }
}

fn cd(args: &[String], err: &mut dyn Write) -> io::Result<()> {
    let home_dir = env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let path = match args.first() {
        None => PathBuf::from(&home_dir),
        Some(arg) => match arg.strip_prefix('~') {
            Some(rest) => Path::new(&home_dir).join(rest),
            None => PathBuf::from(arg),
        },
    };

    if env::set_current_dir(&path).is_err() {
        let display_path = args.first().map_or("~", String::as_str);
        writeln!(err, "cd: {}: No such file or directory", display_path)?;
    }
    Ok(())
}

fn history(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> io::Result<()> {
    let history = &mut shell.history;
    let path = args.get(1);
    // File errors are silently ignored, as before.
    match (args.first().map(String::as_str), path) {
        (Some("-r"), Some(path)) => drop(history.read_from(path)),
        (Some("-w"), Some(path)) => drop(history.write_to(path)),
        (Some("-a"), Some(path)) => drop(history.append_to(path)),
        (Some("-r" | "-w" | "-a"), None) => {}
        (arg, _) => {
            let count = arg.and_then(|s| s.parse().ok());
            history.print(count, out)?;
        }
    }
    Ok(())
}
