use std::fs;
use std::fs::File;

pub fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut inside_single_quote = false;
    let mut inside_double_quote = false;

    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\'' if !inside_double_quote => {
                inside_single_quote = !inside_single_quote;
                // Note: We don't push the quote itself to the token
            }
            '"' if !inside_single_quote => {
                inside_double_quote = !inside_double_quote;
            }
            '\\' if !inside_single_quote => {
                if let Some(&next_c) = chars.peek() {
                    if inside_double_quote {
                        // Inside double quotes, only specific chars are escaped
                        if next_c == '\\' || next_c == '"' || next_c == '$' || next_c == '\n' {
                            current.push(chars.next().unwrap());
                        } else {
                            current.push('\\');
                        }
                    } else {
                        // Outside quotes, backslash escapes the very next char
                        current.push(chars.next().unwrap());
                    }
                }
            }
            ' ' if !inside_single_quote && !inside_double_quote => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Splits a command line into the stages of a pipeline.
pub fn parse_pipeline(line: &str) -> Vec<CommandContext> {
    line.split('|')
        .map(|segment| CommandContext::parse(tokenize(segment)))
        .collect()
}

pub struct CommandContext {
    pub argv: Vec<String>,
    pub stdout_file: Option<File>,
    pub stderr_file: Option<File>,
}

impl CommandContext {
    pub fn parse(tokens: Vec<String>) -> Self {
        let mut final_argv = Vec::new();
        let mut stdout_path = None;
        let mut stderr_path = None;
        let mut append_stdout = false;
        let mut append_stderr = false;

        let mut i = 0;
        while i < tokens.len() {
            match tokens[i].as_str() {
                ">" | "1>" => {
                    stdout_path = tokens.get(i + 1).cloned();
                    append_stdout = false;
                    i += 2;
                }
                ">>" | "1>>" => {
                    stdout_path = tokens.get(i + 1).cloned();
                    append_stdout = true;
                    i += 2;
                }
                "2>" => {
                    stderr_path = tokens.get(i + 1).cloned();
                    append_stderr = false;
                    i += 2;
                }
                "2>>" => {
                    stderr_path = tokens.get(i + 1).cloned();
                    append_stderr = true;
                    i += 2;
                }
                _ => {
                    final_argv.push(tokens[i].clone());
                    i += 1;
                }
            }
        }

        let open_file = |path: String, append: bool| {
            fs::OpenOptions::new()
                .write(true)
                .create(true)
                .append(append)
                .truncate(!append)
                .open(path)
                .ok()
        };

        Self {
            argv: final_argv,
            stdout_file: stdout_path.and_then(|p| open_file(p, append_stdout)),
            stderr_file: stderr_path.and_then(|p| open_file(p, append_stderr)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_dir;
    use std::io::Write;

    fn toks(input: &str) -> Vec<String> {
        tokenize(input)
    }

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn splits_on_runs_of_spaces() {
        assert_eq!(toks("  echo  hello   world "), ["echo", "hello", "world"]);
    }

    #[test]
    fn empty_input_has_no_tokens() {
        assert!(toks("").is_empty());
        assert!(toks("   ").is_empty());
    }

    #[test]
    fn quotes_keep_spaces_and_are_removed() {
        assert_eq!(toks(r#"echo 'a  b' "c  d""#), ["echo", "a  b", "c  d"]);
    }

    #[test]
    fn adjacent_quoted_parts_join_into_one_token() {
        assert_eq!(toks(r#"'a'"b"c"#), ["abc"]);
    }

    #[test]
    fn quotes_of_the_other_kind_are_literal() {
        assert_eq!(toks(r#""it's" 'say "hi"'"#), ["it's", r#"say "hi""#]);
    }

    #[test]
    fn backslash_outside_quotes_escapes_next_char() {
        assert_eq!(toks(r"a\ b \n \\"), ["a b", "n", r"\"]);
    }

    #[test]
    fn backslash_in_single_quotes_is_literal() {
        assert_eq!(toks(r"'a\nb'"), [r"a\nb"]);
    }

    #[test]
    fn backslash_in_double_quotes_escapes_only_special_chars() {
        assert_eq!(toks(r#""a\"b" "c\\d" "e\nf""#), [r#"a"b"#, r"c\d", r"e\nf"]);
    }

    #[test]
    fn plain_command_has_no_redirects() {
        let ctx = CommandContext::parse(strings(&["ls", "-l"]));
        assert_eq!(ctx.argv, ["ls", "-l"]);
        assert!(ctx.stdout_file.is_none());
        assert!(ctx.stderr_file.is_none());
    }

    #[test]
    fn redirect_words_are_removed_from_argv() {
        let dir = temp_dir("parse-argv");
        for op in [">", "1>", ">>", "1>>", "2>", "2>>"] {
            let target = dir.join("f").display().to_string();
            let ctx = CommandContext::parse(strings(&["echo", "a", op, &target, "b"]));
            assert_eq!(ctx.argv, ["echo", "a", "b"], "operator {op}");
        }
    }

    #[test]
    fn stdout_and_stderr_targets_are_opened() {
        let dir = temp_dir("parse-targets");
        let out = dir.join("out").display().to_string();
        let err = dir.join("err").display().to_string();

        let ctx = CommandContext::parse(strings(&["x", "1>", &out]));
        assert!(ctx.stdout_file.is_some() && ctx.stderr_file.is_none());

        let ctx = CommandContext::parse(strings(&["x", "2>", &err]));
        assert!(ctx.stdout_file.is_none() && ctx.stderr_file.is_some());
    }

    /// Writes "new" through a redirect `op` to a file that held "old".
    fn redirect_into_existing(op: &str) -> String {
        let dir = temp_dir(&format!("parse-mode-{}", op.replace('>', "gt")));
        let path = dir.join("f");
        fs::write(&path, "old\n").unwrap();
        let ctx = CommandContext::parse(strings(&["x", op, path.to_str().unwrap()]));
        let mut file = ctx.stdout_file.or(ctx.stderr_file).unwrap();
        writeln!(file, "new").unwrap();
        fs::read_to_string(&path).unwrap()
    }

    #[test]
    fn single_redirect_truncates() {
        assert_eq!(redirect_into_existing(">"), "new\n");
        assert_eq!(redirect_into_existing("2>"), "new\n");
    }

    #[test]
    fn double_redirect_appends() {
        assert_eq!(redirect_into_existing(">>"), "old\nnew\n");
        assert_eq!(redirect_into_existing("2>>"), "old\nnew\n");
    }

    #[test]
    fn redirect_without_target_is_dropped() {
        let ctx = CommandContext::parse(strings(&["echo", "hi", ">"]));
        assert_eq!(ctx.argv, ["echo", "hi"]);
        assert!(ctx.stdout_file.is_none());
    }

    #[test]
    fn pipeline_splits_into_stages() {
        let stages = parse_pipeline("echo a b | tr a-z A-Z|cat");
        let argvs: Vec<_> = stages.iter().map(|s| s.argv.clone()).collect();
        assert_eq!(
            argvs,
            [
                strings(&["echo", "a", "b"]),
                strings(&["tr", "a-z", "A-Z"]),
                strings(&["cat"])
            ]
        );
    }
}
