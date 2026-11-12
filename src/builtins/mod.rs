//! Shell builtins, one file per builtin. To add one:
//!   1. create `<name>.rs` with `pub fn run(ctx: &mut Ctx) -> io::Result<Flow>`,
//!   2. add `mod <name>;` below,
//!   3. add one line to [`BUILTINS`].

mod cd;
mod complete;
mod declare;
mod echo;
mod exit;
mod history;
mod jobs;
mod pwd;
mod r#type;

use crate::shell::{Flow, Shell};
use std::io::{self, Write};

/// Everything a builtin may touch while running.
pub struct Ctx<'a> {
    pub shell: &'a mut Shell,
    /// The arguments after the builtin's name.
    pub args: &'a [String],
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
}

/// Runs one builtin. An `Err` (a failed write, e.g. to a closed pipe) is
/// not reported: the shell just carries on.
pub type Handler = fn(&mut Ctx) -> io::Result<Flow>;

/// The builtin table.
const BUILTINS: &[(&str, Handler)] = &[
    ("cd", cd::run),
    ("complete", complete::run),
    ("declare", declare::run),
    ("echo", echo::run),
    ("exit", exit::run),
    ("history", history::run),
    ("jobs", jobs::run),
    ("pwd", pwd::run),
    ("type", r#type::run),
];

fn lookup(name: &str) -> Option<Handler> {
    BUILTINS
        .iter()
        .find(|(builtin, _)| *builtin == name)
        .map(|(_, handler)| *handler)
}

pub fn is_builtin(name: &str) -> bool {
    lookup(name).is_some()
}

/// The names of all builtins, for `type` and command completion.
pub fn names() -> impl Iterator<Item = &'static str> {
    BUILTINS.iter().map(|(name, _)| *name)
}

/// Runs the builtin named by `argv[0]`, writing its output to `out` and its
/// errors to `err`. The caller must check `is_builtin` first.
pub fn run_builtin(
    shell: &mut Shell,
    argv: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Flow {
    let handler = lookup(&argv[0]).unwrap_or_else(|| panic!("{} is not a builtin", argv[0]));
    let mut ctx = Ctx {
        shell,
        args: &argv[1..],
        out,
        err,
    };
    handler(&mut ctx).unwrap_or(Flow::Continue)
}

/// Helpers for unit-testing builtins against in-memory output.
#[cfg(test)]
pub mod test_util {
    use super::run_builtin;
    use crate::shell::{Flow, Shell};

    /// Runs a builtin, returning its flow, stdout and stderr.
    pub fn run(shell: &mut Shell, argv: &[&str]) -> (Flow, String, String) {
        let argv: Vec<String> = argv.iter().map(|a| a.to_string()).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let flow = run_builtin(shell, &argv, &mut out, &mut err);
        (
            flow,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    /// Runs a builtin in a fresh shell and returns its stdout.
    pub fn stdout_of(argv: &[&str]) -> String {
        run(&mut Shell::default(), argv).1
    }

    /// The result of a builtin that printed nothing and didn't exit.
    pub fn quiet() -> (Flow, String, String) {
        (Flow::Continue, String::new(), String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_has_no_duplicates() {
        let mut seen: Vec<_> = names().collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), BUILTINS.len());
    }

    #[test]
    fn only_table_entries_are_builtins() {
        assert!(names().all(is_builtin));
        assert!(!is_builtin("ls"));
        assert!(!is_builtin(""));
    }

    /// A writer whose every write fails, like a pipe nobody reads.
    struct Broken;

    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn write_errors_are_swallowed() {
        let argv = ["echo".to_string(), "hi".to_string()];
        let flow = run_builtin(&mut Shell::default(), &argv, &mut Broken, &mut Broken);
        assert_eq!(flow, Flow::Continue);
    }

    #[test]
    #[should_panic(expected = "ls is not a builtin")]
    fn running_a_non_builtin_is_a_bug() {
        run_builtin(
            &mut Shell::default(),
            &["ls".to_string()],
            &mut Broken,
            &mut Broken,
        );
    }
}
