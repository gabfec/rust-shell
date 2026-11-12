use crate::builtins::SHELL_BUILTINS;
use crate::utils::is_executable;
use rustyline::completion::Completer;
use rustyline::{Context, Helper, Highlighter, Hinter, Validator};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::process::{Command, Stdio};

#[derive(Default, Helper, Hinter, Highlighter, Validator)]
pub struct ShellHelper {
    /// Completer scripts registered with `complete -C`, by command name.
    pub completers: BTreeMap<String, String>,
}

impl Completer for ShellHelper {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        Ok(self.complete_at(line, pos))
    }
}

impl ShellHelper {
    /// Completes the word before the cursor at byte offset `pos`. Returns
    /// where the replaced word starts and the candidates for it.
    fn complete_at(&self, line: &str, pos: usize) -> (usize, Vec<String>) {
        let buffer = &line[..pos];
        // If there's a space, we're completing an argument
        if let Some(last_space) = buffer.rfind(' ') {
            let prefix = &buffer[last_space + 1..];
            let start = last_space + 1;

            let command = buffer.split_whitespace().next().unwrap_or_default();
            if let Some(script) = self.completers.get(command) {
                let previous = buffer[..last_space]
                    .split_whitespace()
                    .next_back()
                    .unwrap_or_default();
                let args = [command, prefix, previous];
                return (start, run_completer(script, &args, line, pos));
            }

            let (dir, file_prefix, dir_prefix) = if let Some(slash) = prefix.rfind('/') {
                (
                    &prefix[..slash + 1],
                    &prefix[slash + 1..],
                    &prefix[..slash + 1],
                )
            } else {
                (".", prefix, "")
            };

            let mut matches = Vec::new();
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with(file_prefix) {
                        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        let suffix = if is_dir { "/" } else { " " };
                        matches.push(format!("{}{}{}", dir_prefix, name, suffix));
                    }
                }
            }

            matches.sort();

            return (start, matches);
        }

        // No space: complete command (builtins + PATH)
        let mut matches = Vec::new();

        // Check Builtins
        for builtin in SHELL_BUILTINS {
            if builtin.starts_with(buffer) {
                matches.push(builtin.to_string());
            }
        }

        // Check PATH
        if let Some(path_var) = env::var_os("PATH") {
            for dir in env::split_paths(&path_var) {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name.starts_with(buffer) && is_executable(&entry.path()) {
                            if !matches.contains(&name) {
                                matches.push(name);
                            }
                        }
                    }
                }
            }
        }

        if matches.is_empty() {
            return (0, Vec::new());
        }

        matches.sort();
        matches.dedup();

        if matches.len() == 1 {
            // Single unique match -> Append space
            let mut completion = matches[0].clone();
            completion.push(' ');
            (0, vec![completion])
        } else {
            // Multiple matches -> Find Longest Common Prefix (LCP)
            // Let Rustyline handle the LCP and the Bell
            (0, matches)
        }
    }
}

/// Runs a `complete -C` script with bash's arguments (the command name, the
/// word being completed, and the word before it) and environment
/// (`COMP_LINE`, the whole line, and `COMP_POINT`, the cursor's byte
/// offset), and turns each non-blank line it prints into
/// a candidate. A single candidate gets a trailing space, like a finished
/// word. No candidates (including a script that fails to run) means
/// rustyline rings the bell and leaves the line alone; there is
/// deliberately no fallback to filename completion.
fn run_completer(script: &str, args: &[&str; 3], line: &str, pos: usize) -> Vec<String> {
    let Ok(output) = Command::new(script)
        .args(args)
        .env("COMP_LINE", line)
        .env("COMP_POINT", pos.to_string())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    let mut candidates: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(String::from)
        .collect();
    // Several candidates: rustyline completes their common prefix, or rings
    // the bell and lists them (in this order) on the next TAB.
    candidates.sort();
    candidates.dedup();
    if let [single] = candidates.as_mut_slice() {
        single.push(' ');
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_dir;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    /// Writes an executable shell script and returns its path.
    fn script(dir: &Path, name: &str, body: &str) -> String {
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        wait_until_executable(&path);
        path.display().to_string()
    }

    /// Tests run in parallel threads, so another test's fork can briefly
    /// inherit our write handle to the new script, making exec fail with
    /// ETXTBSY. Once one exec succeeds, nothing holds it open any more.
    fn wait_until_executable(path: &Path) {
        const ETXTBSY: i32 = 26;
        for _ in 0..100 {
            match Command::new(path).stdout(Stdio::null()).status() {
                Err(e) if e.raw_os_error() == Some(ETXTBSY) => {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                _ => return,
            }
        }
        panic!("{} stayed busy", path.display());
    }

    impl ShellHelper {
        /// Completes with the cursor at the end of `buffer`.
        fn complete_buffer(&self, buffer: &str) -> (usize, Vec<String>) {
            self.complete_at(buffer, buffer.len())
        }
    }

    fn helper_with(command: &str, script: String) -> ShellHelper {
        ShellHelper {
            completers: BTreeMap::from([(command.to_string(), script)]),
        }
    }

    #[test]
    fn registered_completer_supplies_a_single_word() {
        let dir = temp_dir("complete-single");
        let helper = helper_with("git", script(&dir, "c", "echo add"));
        assert_eq!(
            helper.complete_buffer("git "),
            (4, vec!["add ".to_string()])
        );
    }

    #[test]
    fn completer_gets_command_current_and_previous_word() {
        let dir = temp_dir("complete-argv");
        let helper = helper_with("git", script(&dir, "c", r#"echo "[$1|$2|$3|$#]""#));
        let complete = |buffer| helper.complete_buffer(buffer).1;
        assert_eq!(complete("git remote set"), ["[git|set|remote|3] "]);
        assert_eq!(complete("git remote "), ["[git||remote|3] "]);
        assert_eq!(complete("git  "), ["[git||git|3] "]);
        assert_eq!(complete("git x"), ["[git|x|git|3] "]);
    }

    #[test]
    fn completer_gets_comp_line_and_comp_point() {
        let dir = temp_dir("complete-env");
        let helper = helper_with(
            "git",
            script(&dir, "c", r#"echo "[$COMP_LINE|$COMP_POINT]""#),
        );
        assert_eq!(helper.complete_buffer("git ad").1, ["[git ad|6] "]);
        // Cursor in the middle: COMP_LINE is still the whole line.
        assert_eq!(
            helper.complete_at("git st --all", 6),
            (4, vec!["[git st --all|6] ".to_string()])
        );
    }

    #[test]
    fn multiple_candidates_are_sorted_without_trailing_space() {
        let dir = temp_dir("complete-multi");
        let helper = helper_with(
            "git",
            script(&dir, "c", "printf 'push\\nadd\\ncommit\\nadd\\n'"),
        );
        assert_eq!(helper.complete_buffer("git ").1, ["add", "commit", "push"]);
    }

    #[test]
    fn empty_completer_output_gives_no_candidates() {
        let dir = temp_dir("complete-empty");
        fs::write(dir.join("somefile"), "").unwrap();
        let helper = helper_with("git", script(&dir, "c", "exit 0"));
        assert_eq!(helper.complete_buffer("git x"), (4, vec![]));
    }

    #[test]
    fn blank_lines_are_not_candidates() {
        let dir = temp_dir("complete-blank");
        let helper = helper_with("git", script(&dir, "c", "echo; echo push; echo '  '"));
        assert_eq!(
            helper.complete_buffer("git "),
            (4, vec!["push ".to_string()])
        );
    }

    #[test]
    fn missing_completer_gives_no_candidates() {
        let helper = helper_with("git", "/no/such/completer".to_string());
        assert_eq!(helper.complete_buffer("git "), (4, vec![]));
    }

    #[test]
    fn completer_is_only_used_for_its_command() {
        let dir = temp_dir("complete-other");
        let helper = helper_with("git", script(&dir, "c", "echo add"));
        assert_ne!(
            helper.complete_buffer("docker "),
            (7, vec!["add ".to_string()])
        );
    }
}
