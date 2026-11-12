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

/// Where an output stream is redirected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: String,
    /// `>>` rather than `>`.
    pub append: bool,
}

/// One command of a pipeline: its words and output redirections.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SimpleCommand {
    pub argv: Vec<String>,
    pub stdout: Option<Target>,
    pub stderr: Option<Target>,
}

/// A parsed command line: `A | B | ... | N`, optionally followed by `&`.
#[derive(Debug, PartialEq, Eq)]
pub struct Pipeline {
    pub stages: Vec<SimpleCommand>,
    /// Run in the background (the line ends with `&`).
    pub background: bool,
    /// The line as typed, without the `&`, as `jobs` shows it.
    pub text: String,
}

/// Parses a command line, expanding variables with `lookup`.
pub fn parse(line: &str, lookup: Lookup) -> Pipeline {
    let mut stages: Vec<SimpleCommand> = line
        .split('|')
        .map(|segment| SimpleCommand::parse(tokenize(segment, lookup)))
        .collect();
    let background = stages
        .last()
        .and_then(|stage| stage.argv.last())
        .is_some_and(|arg| arg == "&");
    if background {
        stages.last_mut().unwrap().argv.pop();
    }
    let text = line.trim_end().trim_end_matches('&').trim_end().to_string();
    Pipeline {
        stages,
        background,
        text,
    }
}

impl SimpleCommand {
    /// Separates redirections from the command's words. The last
    /// redirection of a stream wins; one with no target is dropped.
    fn parse(tokens: Vec<String>) -> Self {
        let mut command = SimpleCommand::default();
        let mut tokens = tokens.into_iter();
        while let Some(token) = tokens.next() {
            let (slot, append) = match token.as_str() {
                ">" | "1>" => (&mut command.stdout, false),
                ">>" | "1>>" => (&mut command.stdout, true),
                "2>" => (&mut command.stderr, false),
                "2>>" => (&mut command.stderr, true),
                _ => {
                    command.argv.push(token);
                    continue;
                }
            };
            *slot = tokens.next().map(|path| Target { path, append });
        }
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn command(words: &[&str]) -> SimpleCommand {
        SimpleCommand::parse(strings(words))
    }

    fn target(path: &str, append: bool) -> Option<Target> {
        Some(Target {
            path: path.into(),
            append,
        })
    }

    #[test]
    fn plain_command_has_no_redirects() {
        let ctx = command(&["ls", "-l"]);
        assert_eq!(ctx.argv, ["ls", "-l"]);
        assert_eq!((ctx.stdout, ctx.stderr), (None, None));
    }

    #[test]
    fn redirects_are_taken_out_of_argv() {
        for (op, stdout, stderr) in [
            (">", target("f", false), None),
            ("1>", target("f", false), None),
            (">>", target("f", true), None),
            ("1>>", target("f", true), None),
            ("2>", None, target("f", false)),
            ("2>>", None, target("f", true)),
        ] {
            let parsed = command(&["echo", "a", op, "f", "b"]);
            let expected = SimpleCommand {
                argv: strings(&["echo", "a", "b"]),
                stdout,
                stderr,
            };
            assert_eq!(parsed, expected, "operator {op}");
        }
    }

    #[test]
    fn last_redirect_of_a_stream_wins() {
        let parsed = command(&["x", ">", "a", "2>", "e", ">>", "b"]);
        assert_eq!(parsed.stdout, target("b", true));
        assert_eq!(parsed.stderr, target("e", false));
    }

    #[test]
    fn redirect_without_target_is_dropped() {
        let parsed = command(&["echo", "hi", ">"]);
        assert_eq!(parsed.argv, ["echo", "hi"]);
        assert_eq!(parsed.stdout, None);
    }

    #[test]
    fn pipeline_splits_into_stages() {
        let pipeline = parse("echo a b | tr a-z A-Z|cat", &no_vars);
        let argvs: Vec<_> = pipeline.stages.iter().map(|s| s.argv.clone()).collect();
        assert_eq!(
            argvs,
            [
                strings(&["echo", "a", "b"]),
                strings(&["tr", "a-z", "A-Z"]),
                strings(&["cat"])
            ]
        );
        assert!(!pipeline.background);
    }

    #[test]
    fn trailing_ampersand_means_background() {
        let pipeline = parse("sleep  5 | cat &  ", &no_vars);
        assert!(pipeline.background);
        assert_eq!(pipeline.stages[1].argv, ["cat"]);
        assert_eq!(pipeline.text, "sleep  5 | cat");
    }

    #[test]
    fn text_is_the_line_as_typed() {
        assert_eq!(parse("echo  'a  b'", &no_vars).text, "echo  'a  b'");
    }
}
