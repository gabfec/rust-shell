use super::Ctx;
use crate::shell::Flow;
use std::io::{self, Write};

/// `complete -C <script> <name>...` registers a completer script,
/// `complete -p [<name>...]` prints registrations, and
/// `complete -r [<name>...]` removes them (all of them without names).
pub fn run(ctx: &mut Ctx) -> io::Result<Flow> {
    let completions = &mut ctx.shell.completions;
    let mut print = false;
    let mut remove = false;
    let mut script = None;
    let mut names = Vec::new();
    let mut args = ctx.args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" => print = true,
            "-r" => remove = true,
            "-C" => match args.next() {
                Some(path) => script = Some(path),
                None => {
                    writeln!(ctx.err, "complete: -C: option requires an argument")?;
                    return Ok(Flow::Continue);
                }
            },
            _ => names.push(arg),
        }
    }

    if remove {
        if names.is_empty() {
            completions.clear();
        }
        for name in names {
            // Removing a name that was never registered is not an error.
            completions.remove(name);
        }
    } else if print {
        if names.is_empty() {
            for (name, script) in completions.iter() {
                print_completion(name, script, ctx.out)?;
            }
        }
        for name in names {
            match completions.get(name) {
                Some(script) => print_completion(name, script, ctx.out)?,
                None => writeln!(ctx.err, "complete: {}: no completion specification", name)?,
            }
        }
    } else if let Some(script) = script {
        for name in names {
            completions.insert(name.clone(), script.clone());
        }
    }
    Ok(Flow::Continue)
}

/// Prints a registration in the form `complete` accepts as input.
fn print_completion(name: &str, script: &str, out: &mut dyn Write) -> io::Result<()> {
    let quoted = script.replace('\'', r"'\''");
    writeln!(out, "complete -C '{}' {}", quoted, name)
}

#[cfg(test)]
mod tests {
    use crate::builtins::test_util::{quiet, run};
    use crate::shell::Shell;

    #[test]
    fn p_reports_missing_specification() {
        let (_, out, err) = run(&mut Shell::default(), &["complete", "-p", "git"]);
        assert_eq!(out, "");
        assert_eq!(err, "complete: git: no completion specification\n");
    }

    #[test]
    fn c_registers_and_p_prints_normalized() {
        let mut shell = Shell::default();
        let registered = run(&mut shell, &["complete", "-C", "/bin/git-done", "git"]);
        assert_eq!(registered, quiet());
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
    fn p_mixes_found_and_missing() {
        let mut shell = Shell::default();
        run(&mut shell, &["complete", "-C", "/c", "git"]);
        let (_, out, err) = run(&mut shell, &["complete", "-p", "git", "nope"]);
        assert_eq!(out, "complete -C '/c' git\n");
        assert_eq!(err, "complete: nope: no completion specification\n");
    }

    #[test]
    fn r_removes_registrations_silently() {
        let mut shell = Shell::default();
        for name in ["git", "docker", "svn"] {
            run(&mut shell, &["complete", "-C", "/c", name]);
        }
        assert_eq!(run(&mut shell, &["complete", "-r", "git"]), quiet());
        assert_eq!(
            run(&mut shell, &["complete", "-r", "never-registered"]),
            quiet()
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
    fn c_without_script_is_an_error() {
        let mut shell = Shell::default();
        let (_, out, err) = run(&mut shell, &["complete", "-C"]);
        assert_eq!(out, "");
        assert_eq!(err, "complete: -C: option requires an argument\n");
        assert!(shell.completions.is_empty());
    }
}
