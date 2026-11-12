use crate::jobs::print_jobs;
use crate::shell::{Flow, Shell};
use crate::utils::find_in_path;
use std::env;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const SHELL_BUILTINS: &[&str] = &[
    "exit", "echo", "type", "pwd", "cd", "history", "jobs", "complete", "declare",
];

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
        "complete" => complete(shell, args, out, err),
        "declare" => Ok(()),
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

/// `complete -C <script> <name>...` registers a completer script,
/// `complete -p [<name>...]` prints registrations, and
/// `complete -r [<name>...]` removes them (all of them without names).
fn complete(
    shell: &mut Shell,
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> io::Result<()> {
    let mut print = false;
    let mut remove = false;
    let mut script = None;
    let mut names = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" => print = true,
            "-r" => remove = true,
            "-C" => match args.next() {
                Some(path) => script = Some(path),
                None => return writeln!(err, "complete: -C: option requires an argument"),
            },
            _ => names.push(arg),
        }
    }

    if remove {
        if names.is_empty() {
            shell.completions.clear();
        }
        for name in names {
            // Removing a name that was never registered is not an error.
            shell.completions.remove(name);
        }
    } else if print {
        if names.is_empty() {
            for (name, script) in &shell.completions {
                print_completion(name, script, out)?;
            }
        }
        for name in names {
            match shell.completions.get(name) {
                Some(script) => print_completion(name, script, out)?,
                None => writeln!(err, "complete: {}: no completion specification", name)?,
            }
        }
    } else if let Some(script) = script {
        for name in names {
            shell.completions.insert(name.clone(), script.clone());
        }
    }
    Ok(())
}

/// Prints a registration in the form `complete` accepts as input.
fn print_completion(name: &str, script: &str, out: &mut dyn Write) -> io::Result<()> {
    let quoted = script.replace('\'', r"'\''");
    writeln!(out, "complete -C '{}' {}", quoted, name)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_dir;
    use std::fs;

    /// Runs a builtin, returning its flow, stdout and stderr.
    fn run(shell: &mut Shell, argv: &[&str]) -> (Flow, String, String) {
        let argv: Vec<String> = argv.iter().map(|a| a.to_string()).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let flow = run_builtin(shell, &argv, &mut out, &mut err);
        (
            flow,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    fn stdout_of(argv: &[&str]) -> String {
        run(&mut Shell::default(), argv).1
    }

    #[test]
    fn exit_stops_the_shell_silently() {
        assert_eq!(
            run(&mut Shell::default(), &["exit"]),
            (Flow::Exit, String::new(), String::new())
        );
    }

    #[test]
    fn echo_joins_arguments() {
        assert_eq!(stdout_of(&["echo", "a", "b  c"]), "a b  c\n");
        assert_eq!(stdout_of(&["echo"]), "\n");
    }

    #[test]
    fn type_recognises_builtins() {
        for builtin in SHELL_BUILTINS {
            assert_eq!(
                stdout_of(&["type", builtin]),
                format!("{builtin} is a shell builtin\n")
            );
        }
    }

    #[test]
    fn type_finds_executables_on_path() {
        let out = stdout_of(&["type", "cat"]);
        assert!(
            out.starts_with("cat is /") && out.ends_with("/cat\n"),
            "{out:?}"
        );
    }

    #[test]
    fn type_reports_unknown_commands() {
        assert_eq!(
            stdout_of(&["type", "no_such_cmd_xyz"]),
            "no_such_cmd_xyz: not found\n"
        );
        assert_eq!(stdout_of(&["type"]), "\n");
    }

    #[test]
    fn pwd_prints_current_directory() {
        let expected = format!("{}\n", env::current_dir().unwrap().display());
        assert_eq!(stdout_of(&["pwd"]), expected);
    }

    #[test]
    fn cd_to_missing_directory_reports_on_stderr() {
        let (flow, out, err) = run(&mut Shell::default(), &["cd", "/no/such/dir"]);
        assert_eq!(flow, Flow::Continue);
        assert_eq!(out, "");
        assert_eq!(err, "cd: /no/such/dir: No such file or directory\n");
    }

    #[test]
    fn history_prints_and_limits() {
        let mut shell = Shell::default();
        for line in ["a", "b", "c"] {
            shell.history.push(line);
        }
        assert_eq!(run(&mut shell, &["history", "2"]).1, "    2  b\n    3  c\n");
        assert_eq!(run(&mut shell, &["history"]).1.lines().count(), 3);
    }

    #[test]
    fn history_file_flags_round_trip() {
        let dir = temp_dir("builtin-history");
        let path = dir.join("h").display().to_string();

        let mut shell = Shell::default();
        shell.history.push("a");
        run(&mut shell, &["history", "-w", &path]);
        shell.history.push("b");
        run(&mut shell, &["history", "-a", &path]);
        assert_eq!(fs::read_to_string(&path).unwrap(), "a\nb\n");

        let mut other = Shell::default();
        run(&mut other, &["history", "-r", &path]);
        assert_eq!(other.history.entries(), ["a", "b"]);
    }

    #[test]
    fn history_flag_without_path_does_nothing() {
        let mut shell = Shell::default();
        shell.history.push("a");
        assert_eq!(
            run(&mut shell, &["history", "-r"]),
            (Flow::Continue, String::new(), String::new())
        );
        assert_eq!(shell.history.entries(), ["a"]);
    }

    #[test]
    fn complete_p_reports_missing_specification() {
        let (_, out, err) = run(&mut Shell::default(), &["complete", "-p", "git"]);
        assert_eq!(out, "");
        assert_eq!(err, "complete: git: no completion specification\n");
    }

    #[test]
    fn complete_c_registers_and_p_prints_normalized() {
        let mut shell = Shell::default();
        let registered = run(&mut shell, &["complete", "-C", "/bin/git-done", "git"]);
        assert_eq!(registered, (Flow::Continue, String::new(), String::new()));
        run(&mut shell, &["complete", "-C", "/it's", "docker", "podman"]);

        assert_eq!(
            run(&mut shell, &["complete", "-p", "git"]).1,
            "complete -C '/bin/git-done' git\n"
        );
        assert_eq!(
            run(&mut shell, &["complete", "-p", "podman"]).1,
            "complete -C '/it'\\''s' podman\n"
        );
        // Re-registering replaces the script.
        run(&mut shell, &["complete", "-C", "/new", "git"]);
        assert_eq!(
            run(&mut shell, &["complete", "-p"]).1,
            "complete -C '/it'\\''s' docker\n\
             complete -C '/new' git\n\
             complete -C '/it'\\''s' podman\n"
        );
    }

    #[test]
    fn complete_p_mixes_found_and_missing() {
        let mut shell = Shell::default();
        run(&mut shell, &["complete", "-C", "/c", "git"]);
        let (_, out, err) = run(&mut shell, &["complete", "-p", "git", "nope"]);
        assert_eq!(out, "complete -C '/c' git\n");
        assert_eq!(err, "complete: nope: no completion specification\n");
    }

    #[test]
    fn complete_r_removes_registrations_silently() {
        let mut shell = Shell::default();
        for name in ["git", "docker", "svn"] {
            run(&mut shell, &["complete", "-C", "/c", name]);
        }
        let quiet = (Flow::Continue, String::new(), String::new());
        assert_eq!(run(&mut shell, &["complete", "-r", "git"]), quiet);
        assert_eq!(
            run(&mut shell, &["complete", "-r", "never-registered"]),
            quiet
        );
        assert_eq!(
            run(&mut shell, &["complete", "-p", "git"]).2,
            "complete: git: no completion specification\n"
        );
        assert_eq!(shell.completions.len(), 2);

        run(&mut shell, &["complete", "-r"]);
        assert!(shell.completions.is_empty());
    }

    #[test]
    fn jobs_with_no_jobs_prints_nothing() {
        assert_eq!(stdout_of(&["jobs"]), "");
    }
}
