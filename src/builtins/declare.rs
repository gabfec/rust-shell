use super::Ctx;
use crate::parser::is_valid_name;
use crate::shell::Flow;
use std::io::{self, Write};

/// `declare <name>=<value>...` sets variables, and
/// `declare -p [<name>...]` prints them (all of them without names).
pub fn run(ctx: &mut Ctx) -> io::Result<Flow> {
    let variables = &mut ctx.shell.variables;
    let print = ctx.args.iter().any(|arg| arg == "-p");
    // Other attribute flags (-i, -x, ...) are not supported and ignored.
    let words = ctx.args.iter().filter(|arg| !arg.starts_with('-'));

    if print {
        let mut words = words.peekable();
        if words.peek().is_none() {
            for (name, value) in variables.iter() {
                print_variable(name, value, ctx.out)?;
            }
        }
        for name in words {
            match variables.get(name) {
                Some(value) => print_variable(name, value, ctx.out)?,
                None => writeln!(ctx.err, "declare: {}: not found", name)?,
            }
        }
        return Ok(Flow::Continue);
    }

    for word in words {
        let (name, value) = word.split_once('=').unwrap_or((word, ""));
        if !is_valid_name(name) {
            writeln!(ctx.err, "declare: `{}': not a valid identifier", word)?;
        } else if word.contains('=') {
            variables.insert(name.to_string(), value.to_string());
        }
    }
    Ok(Flow::Continue)
}

/// Prints a variable the way bash's `declare -p` does, escaping the
/// characters that are special inside double quotes.
fn print_variable(name: &str, value: &str, out: &mut dyn Write) -> io::Result<()> {
    let mut escaped = String::with_capacity(value.len());
    for c in value.chars() {
        if matches!(c, '\\' | '"' | '$' | '`') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    writeln!(out, "declare -- {}=\"{}\"", name, escaped)
}

#[cfg(test)]
mod tests {
    use crate::builtins::test_util::{quiet, run};
    use crate::shell::{Flow, Shell};

    #[test]
    fn p_reports_missing_variable() {
        let (_, out, err) = run(&mut Shell::default(), &["declare", "-p", "missing"]);
        assert_eq!(out, "");
        assert_eq!(err, "declare: missing: not found\n");
    }

    #[test]
    fn sets_and_overwrites_variables() {
        let mut shell = Shell::default();
        assert_eq!(run(&mut shell, &["declare", "foo=bar", "empty="]), quiet());
        assert_eq!(
            run(&mut shell, &["declare", "-p", "foo"]).1,
            "declare -- foo=\"bar\"\n"
        );
        assert_eq!(
            run(&mut shell, &["declare", "-p", "empty"]).1,
            "declare -- empty=\"\"\n"
        );

        run(&mut shell, &["declare", "foo=bar2"]);
        assert_eq!(
            run(&mut shell, &["declare", "-p", "foo"]).1,
            "declare -- foo=\"bar2\"\n"
        );
    }

    #[test]
    fn value_keeps_later_equals_signs() {
        let mut shell = Shell::default();
        run(&mut shell, &["declare", "url=a=b=c"]);
        assert_eq!(shell.variables["url"], "a=b=c");
    }

    #[test]
    fn p_escapes_like_bash() {
        let mut shell = Shell::default();
        run(&mut shell, &["declare", r#"v=a"b\c$d`e f"#]);
        assert_eq!(
            run(&mut shell, &["declare", "-p", "v"]).1,
            "declare -- v=\"a\\\"b\\\\c\\$d\\`e f\"\n"
        );
    }

    #[test]
    fn p_without_names_prints_all_sorted() {
        let mut shell = Shell::default();
        run(&mut shell, &["declare", "b=2", "a=1"]);
        assert_eq!(
            run(&mut shell, &["declare", "-p"]).1,
            "declare -- a=\"1\"\ndeclare -- b=\"2\"\n"
        );
    }

    #[test]
    fn rejects_invalid_names() {
        let mut shell = Shell::default();
        for word in ["23=x", "pear-grape=orange", "=x", "a b=c", "5raspberry"] {
            let (flow, out, err) = run(&mut shell, &["declare", word]);
            assert_eq!(flow, Flow::Continue);
            assert_eq!(out, "");
            assert_eq!(err, format!("declare: `{word}': not a valid identifier\n"));
        }
        assert!(shell.variables.is_empty());
    }

    #[test]
    fn accepts_underscores_and_digits_after_the_first_char() {
        let mut shell = Shell::default();
        let (_, _, err) = run(&mut shell, &["declare", "_FOO=bar", "x9_y=1", "_=u"]);
        assert_eq!(err, "");
        assert_eq!(shell.variables.len(), 3);
    }

    #[test]
    fn rejects_bad_words_but_keeps_good_ones() {
        let mut shell = Shell::default();
        run(&mut shell, &["declare", "ok=1", "1bad=2", "fine=3"]);
        assert_eq!(shell.variables.keys().collect::<Vec<_>>(), ["fine", "ok"]);
    }

    #[test]
    fn name_without_value_is_accepted_but_not_stored() {
        let mut shell = Shell::default();
        assert_eq!(run(&mut shell, &["declare", "later"]), quiet());
        assert!(shell.variables.is_empty());
    }

    #[test]
    fn other_flags_are_ignored() {
        let mut shell = Shell::default();
        assert_eq!(run(&mut shell, &["declare", "-x", "a=1"]), quiet());
        assert_eq!(shell.variables["a"], "1");
    }
}
