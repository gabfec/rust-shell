use super::Ctx;
use crate::jobs::print_jobs;
use crate::shell::Flow;
use std::io;

/// `jobs` lists background jobs, reporting (and forgetting) finished ones.
pub fn run(ctx: &mut Ctx) -> io::Result<Flow> {
    print_jobs(&ctx.shell.jobs.reap(), true, ctx.out)?;
    Ok(Flow::Continue)
}

#[cfg(test)]
mod tests {
    use crate::builtins::test_util::stdout_of;

    #[test]
    fn prints_nothing_without_jobs() {
        assert_eq!(stdout_of(&["jobs"]), "");
    }
}
