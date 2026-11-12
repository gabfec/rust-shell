use super::Ctx;
use crate::shell::Flow;
use std::{env, io};

/// `pwd` prints the current directory.
pub fn run(ctx: &mut Ctx) -> io::Result<Flow> {
    match env::current_dir() {
        Ok(dir) => writeln!(ctx.out, "{}", dir.display())?,
        Err(e) => writeln!(ctx.err, "pwd: {}", e)?,
    }
    Ok(Flow::Continue)
}

#[cfg(test)]
mod tests {
    use crate::builtins::test_util::stdout_of;
    use std::env;

    #[test]
    fn prints_current_directory() {
        let expected = format!("{}\n", env::current_dir().unwrap().display());
        assert_eq!(stdout_of(&["pwd"]), expected);
    }
}
