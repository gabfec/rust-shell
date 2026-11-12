use super::Ctx;
use crate::shell::Flow;
use std::io;

/// `history [<n>]` prints the last `n` entries (all without `n`);
/// `history -r|-w|-a <file>` reads, writes or appends the history file.
pub fn run(ctx: &mut Ctx) -> io::Result<Flow> {
    let history = &mut ctx.shell.history;
    let args = ctx.args;
    // File errors are silently ignored.
    match (args.first().map(String::as_str), args.get(1)) {
        (Some("-r"), Some(path)) => drop(history.read_from(path)),
        (Some("-w"), Some(path)) => drop(history.write_to(path)),
        (Some("-a"), Some(path)) => drop(history.append_to(path)),
        (Some("-r" | "-w" | "-a"), None) => {}
        (arg, _) => {
            let count = arg.and_then(|s| s.parse().ok());
            history.print(count, ctx.out)?;
        }
    }
    Ok(Flow::Continue)
}

#[cfg(test)]
mod tests {
    use crate::builtins::test_util::{quiet, run};
    use crate::shell::Shell;
    use crate::test_util::temp_dir;
    use std::fs;

    fn shell_with(entries: &[&str]) -> Shell {
        let mut shell = Shell::default();
        for entry in entries {
            shell.history.push(entry);
        }
        shell
    }

    #[test]
    fn prints_all_or_last_n() {
        let mut shell = shell_with(&["a", "b", "c"]);
        assert_eq!(run(&mut shell, &["history", "2"]).1, "    2  b\n    3  c\n");
        assert_eq!(run(&mut shell, &["history"]).1.lines().count(), 3);
    }

    #[test]
    fn file_flags_round_trip() {
        let dir = temp_dir("builtin-history");
        let path = dir.join("h").display().to_string();

        let mut shell = shell_with(&["a"]);
        run(&mut shell, &["history", "-w", &path]);
        shell.history.push("b");
        run(&mut shell, &["history", "-a", &path]);
        assert_eq!(fs::read_to_string(&path).unwrap(), "a\nb\n");

        let mut other = Shell::default();
        run(&mut other, &["history", "-r", &path]);
        assert_eq!(other.history.entries(), ["a", "b"]);
    }

    #[test]
    fn flag_without_path_does_nothing() {
        let mut shell = shell_with(&["a"]);
        assert_eq!(run(&mut shell, &["history", "-r"]), quiet());
        assert_eq!(shell.history.entries(), ["a"]);
    }
}
