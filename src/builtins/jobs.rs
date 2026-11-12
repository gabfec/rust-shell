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
    use crate::builtins::test_util::{run, stdout_of};
    use crate::shell::Shell;
    use std::process::{Command, Stdio};

    #[test]
    fn prints_nothing_without_jobs() {
        assert_eq!(stdout_of(&["jobs"]), "");
    }

    #[test]
    fn lists_running_and_reaps_finished_jobs() {
        let mut shell = Shell::default();
        // Runs until the shell (and so its stdin pipe) is dropped.
        let running = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let mut finished = Command::new("true").spawn().unwrap();
        finished.wait().unwrap();
        shell.jobs.add("cat".into(), vec![running]);
        shell.jobs.add("true".into(), vec![finished]);

        assert_eq!(
            run(&mut shell, &["jobs"]).1,
            format!("[1]-  {:<24}cat &\n[2]+  {:<24}true\n", "Running", "Done")
        );
        assert_eq!(
            run(&mut shell, &["jobs"]).1,
            format!("[1]+  {:<24}cat &\n", "Running")
        );
    }
}
