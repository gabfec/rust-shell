use std::fs;
use std::fs::File;
use std::iter::Peekable;
use std::str::Chars;

/// Looks up a variable's value for `$NAME` expansion.
pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Splits a command into words, removing quotes and backslashes and
/// expanding `$NAME` outside single quotes. A word that ends up empty is
/// dropped unless it was quoted: `$unset` vanishes, `"$unset"` and `''`
/// are empty arguments.
pub fn tokenize(input: &str, lookup: Lookup) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut inside_single_quote = false;
    let mut inside_double_quote = false;
    // Whether the current word contains quotes, so it is kept even if empty.
    let mut quoted = false;

    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\'' if !inside_double_quote => {
                inside_single_quote = !inside_single_quote;
                quoted = true;
                // Note: We don't push the quote itself to the token
            }
            '"' if !inside_single_quote => {
                inside_double_quote = !inside_double_quote;
                quoted = true;
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
            '$' if !inside_single_quote => {
                expand_variable(&mut chars, lookup, &mut current);
            }
            ' ' if !inside_single_quote && !inside_double_quote => {
                if !current.is_empty() || quoted {
                    tokens.push(std::mem::take(&mut current));
                }
                quoted = false;
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() || quoted {
        tokens.push(current);
    }
    tokens
}

/// Expands the variable reference following a `$` (`NAME` or `{NAME}`)
/// into `out`. A `$` not followed by either is kept literally, as is a
/// `${...}` that is unterminated or doesn't hold a valid name.
fn expand_variable(chars: &mut Peekable<Chars>, lookup: Lookup, out: &mut String) {
    if chars.next_if_eq(&'{').is_some() {
        let mut inner = String::new();
        let closed = loop {
            match chars.next() {
                Some('}') => break true,
                Some(c) => inner.push(c),
                None => break false,
            }
        };
        if closed && is_valid_name(&inner) {
            out.push_str(&lookup(&inner).unwrap_or_default());
        } else {
            out.push_str("${");
            out.push_str(&inner);
            if closed {
                out.push('}');
            }
        }
        return;
    }

    let mut name = String::new();
    if chars
        .peek()
        .is_some_and(|c| c.is_ascii_alphabetic() || *c == '_')
    {
        while let Some(c) = chars.next_if(|c| c.is_ascii_alphanumeric() || *c == '_') {
            name.push(c);
        }
    }
    if name.is_empty() {
        out.push('$');
    } else {
        out.push_str(&lookup(&name).unwrap_or_default());
    }
}

/// Whether `name` can name a variable: a letter or `_`, followed by
/// letters, digits or `_`.
pub fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Splits a command line into the stages of a pipeline.
pub fn parse_pipeline(line: &str, lookup: Lookup) -> Vec<CommandContext> {
    line.split('|')
        .map(|segment| CommandContext::parse(tokenize(segment, lookup)))
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

    fn no_vars(_: &str) -> Option<String> {
        None
    }

    fn toks(input: &str) -> Vec<String> {
        tokenize(input, &no_vars)
    }

    fn vars(name: &str) -> Option<String> {
        match name {
            "Item" => Some("widget".into()),
            "Foo1" => Some("Bar2".into()),
            "spaced" => Some("a  b".into()),
            _ => None,
        }
    }

    fn expand(input: &str) -> Vec<String> {
        tokenize(input, &vars)
    }

    #[test]
    fn dollar_name_expands() {
        assert_eq!(expand("echo $Item $Foo1"), ["echo", "widget", "Bar2"]);
    }

    #[test]
    fn name_ends_at_first_non_name_char() {
        assert_eq!(
            expand("$Item.txt pre_$Foo1-x"),
            ["widget.txt", "pre_Bar2-x"]
        );
        // `_` and digits belong to the name, so this is the unset `Item_1`.
        assert_eq!(expand("a$Item_1"), ["a"]);
    }

    #[test]
    fn braced_name_expands_and_can_be_followed_by_name_chars() {
        assert_eq!(
            expand("stock_${Item}_id ${Foo1}"),
            ["stock_widget_id", "Bar2"]
        );
        assert_eq!(expand(r#""${Item}s" '${Item}'"#), ["widgets", "${Item}"]);
    }

    #[test]
    fn malformed_braces_stay_literal() {
        assert_eq!(expand("a ${Item"), ["a", "${Item"]);
        assert_eq!(expand("${1x} ${}"), ["${1x}", "${}"]);
        assert_eq!(expand(r#""${Foo1 x}""#), ["${Foo1 x}"]);
    }

    #[test]
    fn unquoted_empty_expansion_is_dropped() {
        assert_eq!(
            expand("x ${missing1}end ${Item} ${missing2} $missing3"),
            ["x", "end", "widget"]
        );
    }

    #[test]
    fn quoted_empty_words_are_kept() {
        assert_eq!(
            expand(r#"x "" '' "$missing" "${missing}""#),
            ["x", "", "", "", ""]
        );
        // Quotes inside a word that has other text don't add anything.
        assert_eq!(expand(r#"a"" ''b"#), ["a", "b"]);
    }

    #[test]
    fn unset_variable_is_empty() {
        assert_eq!(expand("echo $missing x"), ["echo", "x"]);
    }

    #[test]
    fn expands_in_double_quotes_but_not_single() {
        assert_eq!(expand(r#""$Item" '$Item'"#), ["widget", "$Item"]);
        // Quoted expansions keep their spaces in one word.
        assert_eq!(expand(r#""<$spaced>""#), ["<a  b>"]);
    }

    #[test]
    fn escaped_or_lone_dollar_is_literal() {
        assert_eq!(
            expand(r#"\$Item "\$Item" $ a$ $1 $-"#),
            ["$Item", "$Item", "$", "a$", "$1", "$-"]
        );
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
        let stages = parse_pipeline("echo a b | tr a-z A-Z|cat", &no_vars);
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
