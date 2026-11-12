use super::{Ctx, is_builtin};
use crate::shell::Flow;
use crate::utils::find_in_path;
use std::io;

/// `type <name>` says whether `name` is a builtin or which executable on
/// `PATH` it runs.
pub fn run(ctx: &mut Ctx) -> io::Result<Flow> {
    let Some(query) = ctx.args.first() else {
        writeln!(ctx.out)?;
        return Ok(Flow::Continue);
    };
    if is_builtin(query) {
        writeln!(ctx.out, "{} is a shell builtin", query)?;
    } else if let Some(full_path) = find_in_path(query) {
        writeln!(ctx.out, "{} is {}", query, full_path)?;
    } else {
        writeln!(ctx.out, "{}: not found", query)?;
    }
    Ok(Flow::Continue)
}

#[cfg(test)]
mod tests {
    use crate::builtins::names;
    use crate::builtins::test_util::stdout_of;

    #[test]
    fn recognises_every_builtin() {
        for builtin in names() {
            assert_eq!(
                stdout_of(&["type", builtin]),
                format!("{builtin} is a shell builtin\n")
            );
        }
    }

    #[test]
    fn finds_executables_on_path() {
        let out = stdout_of(&["type", "cat"]);
        assert!(
            out.starts_with("cat is /") && out.ends_with("/cat\n"),
            "{out:?}"
        );
    }

    #[test]
    fn reports_unknown_commands() {
        assert_eq!(
            stdout_of(&["type", "no_such_cmd_xyz"]),
            "no_such_cmd_xyz: not found\n"
        );
    }

    #[test]
    fn prints_empty_line_without_arguments() {
        assert_eq!(stdout_of(&["type"]), "\n");
    }
}
