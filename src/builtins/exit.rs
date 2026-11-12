use super::Ctx;
use crate::shell::Flow;
use std::io;

/// `exit` leaves the shell. A status argument is accepted but ignored.
pub fn run(_ctx: &mut Ctx) -> io::Result<Flow> {
    Ok(Flow::Exit)
}

#[cfg(test)]
mod tests {
    use crate::builtins::test_util::run;
    use crate::shell::{Flow, Shell};

    #[test]
    fn exits_silently() {
        let exited = (Flow::Exit, String::new(), String::new());
        assert_eq!(run(&mut Shell::default(), &["exit"]), exited);
    }
}
