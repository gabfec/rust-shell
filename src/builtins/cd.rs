use super::Ctx;
use crate::shell::Flow;
use std::path::{Path, PathBuf};
use std::{env, io};

/// `cd [<dir>]` changes directory; no argument, or a leading `~`, means
/// `$HOME`.
pub fn run(ctx: &mut Ctx) -> io::Result<Flow> {
    let home = env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let arg = ctx.args.first().map(String::as_str);
    if env::set_current_dir(target(arg, &home)).is_err() {
        writeln!(
            ctx.err,
            "cd: {}: No such file or directory",
            arg.unwrap_or("~")
        )?;
    }
    Ok(Flow::Continue)
}

/// The directory `cd <arg>` goes to.
fn target(arg: Option<&str>, home: &str) -> PathBuf {
    match arg {
        None => PathBuf::from(home),
        Some(arg) => match arg.strip_prefix('~') {
            Some(rest) => Path::new(home).join(rest),
            None => PathBuf::from(arg),
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::builtins::test_util::run;
    use crate::shell::{Flow, Shell};

    #[test]
    fn missing_directory_is_reported_on_stderr() {
        let (flow, out, err) = run(&mut Shell::default(), &["cd", "/no/such/dir"]);
        assert_eq!(flow, Flow::Continue);
        assert_eq!(out, "");
        assert_eq!(err, "cd: /no/such/dir: No such file or directory\n");
    }
}
