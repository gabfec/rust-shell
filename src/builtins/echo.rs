use super::Ctx;
use crate::shell::Flow;
use std::io;

/// `echo <word>...` prints its arguments separated by spaces.
pub fn run(ctx: &mut Ctx) -> io::Result<Flow> {
    writeln!(ctx.out, "{}", ctx.args.join(" "))?;
    Ok(Flow::Continue)
}

#[cfg(test)]
mod tests {
    use crate::builtins::test_util::stdout_of;

    #[test]
    fn joins_arguments() {
        assert_eq!(stdout_of(&["echo", "a", "b  c"]), "a b  c\n");
    }

    #[test]
    fn prints_empty_line_without_arguments() {
        assert_eq!(stdout_of(&["echo"]), "\n");
    }

    #[test]
    fn keeps_empty_arguments() {
        assert_eq!(stdout_of(&["echo", "", "x"]), " x\n");
    }
}
