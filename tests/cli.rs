//! End-to-end tests: feed a script to the shell binary on stdin and check
//! what it prints. Each test runs in its own temporary directory, which is
//! also $HOME, with $HISTFILE unset unless the test sets it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::{env, process};

fn temp_dir(name: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("rust-shell-cli-{}-{}", process::id(), name));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

struct Run {
    stdout: String,
    stderr: String,
}

fn run_in(dir: &Path, script: &str, histfile: Option<&Path>) -> Run {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_codecrafters-shell"));
    cmd.current_dir(dir)
        .env("HOME", dir)
        .env_remove("HISTFILE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = histfile {
        cmd.env("HISTFILE", path);
    }

    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let Output { stdout, stderr, .. } = child.wait_with_output().unwrap();
    Run {
        stdout: String::from_utf8(stdout).unwrap(),
        stderr: String::from_utf8(stderr).unwrap(),
    }
}

/// Runs `script` in a fresh directory named after the test.
fn run(name: &str, script: &str) -> Run {
    run_in(&temp_dir(name), script, None)
}

#[test]
fn echo_with_quotes() {
    let r = run("echo", "echo 'a  b'   \"c\" d\n");
    assert_eq!(r.stdout, "a  b c d\n");
    assert_eq!(r.stderr, "");
}

#[test]
fn type_builtin_and_unknown() {
    let r = run("type", "type echo\ntype no_such_cmd_xyz\n");
    assert_eq!(
        r.stdout,
        "echo is a shell builtin\nno_such_cmd_xyz: not found\n"
    );
}

#[test]
fn unknown_command_reports_and_continues() {
    let r = run("unknown", "no_such_cmd_xyz arg\necho after\n");
    assert_eq!(r.stdout, "after\n");
    assert_eq!(r.stderr, "no_such_cmd_xyz: not found\n");
}

#[test]
fn exit_stops_reading_commands() {
    let r = run("exit", "echo a\nexit\necho b\n");
    assert_eq!(r.stdout, "a\n");
}

#[test]
fn cd_and_pwd() {
    let dir = temp_dir("cd");
    fs::create_dir(dir.join("sub")).unwrap();
    let r = run_in(&dir, "cd sub\npwd\ncd ~\npwd\ncd /no/such/dir\n", None);

    let real = dir.canonicalize().unwrap();
    assert_eq!(
        r.stdout,
        format!("{}\n{}\n", real.join("sub").display(), real.display())
    );
    assert_eq!(r.stderr, "cd: /no/such/dir: No such file or directory\n");
}

#[test]
fn stdout_redirect_truncates_and_appends() {
    let r = run(
        "redir-out",
        "echo one > f\necho two >> f\ncat f\necho three 1> f\ncat f\n",
    );
    assert_eq!(r.stdout, "one\ntwo\nthree\n");
}

#[test]
fn stderr_redirect() {
    let r = run("redir-err", "ls /no_such_path_xyz 2> err\ncat err\n");
    assert_eq!(r.stderr, "");
    assert!(r.stdout.contains("no_such_path_xyz"), "{:?}", r.stdout);
}

#[test]
fn builtin_output_can_be_redirected() {
    let dir = temp_dir("redir-builtin");
    let r = run_in(&dir, "history 1 > h\ntype echo >> h\njobs > j\n", None);
    assert_eq!(r.stdout, "");
    assert_eq!(
        fs::read_to_string(dir.join("h")).unwrap(),
        "    1  history 1 > h\necho is a shell builtin\n"
    );
    assert_eq!(fs::read_to_string(dir.join("j")).unwrap(), "");
}

#[test]
fn pipeline_of_external_commands() {
    let r = run("pipe-ext", "echo hello | cat | tr a-z A-Z\n");
    assert_eq!(r.stdout, "HELLO\n");
}

#[test]
fn builtin_output_feeds_a_pipe_verbatim() {
    let r = run(
        "pipe-builtin",
        "echo 100% done | cat\ntype exit | tr a-z A-Z\n",
    );
    assert_eq!(r.stdout, "100% done\nEXIT IS A SHELL BUILTIN\n");
}

#[test]
fn builtin_at_the_end_of_a_pipe() {
    let r = run("pipe-builtin-last", "echo ignored | echo last\n");
    assert_eq!(r.stdout, "last\n");
}

#[test]
fn history_inside_a_pipeline() {
    let r = run("pipe-history", "echo a\nhistory | tail -n 1\n");
    assert_eq!(r.stdout, "a\n    2  history | tail -n 1\n");
}

#[test]
fn redirects_inside_a_pipeline() {
    let r = run(
        "pipe-redir",
        "echo piped | cat > out\ncat out\nls /no_such_path_xyz 2> err | cat\ncat err\n",
    );
    assert!(r.stdout.starts_with("piped\n"), "{:?}", r.stdout);
    assert!(r.stdout.contains("no_such_path_xyz"), "{:?}", r.stdout);
    assert_eq!(r.stderr, "");
}

#[test]
fn missing_command_in_a_pipeline_does_not_crash() {
    let r = run("pipe-missing", "no_such_cmd_xyz | cat\necho alive\n");
    assert_eq!(r.stdout, "alive\n");
    assert_eq!(r.stderr, "no_such_cmd_xyz: not found\n");
}

#[test]
fn exit_inside_a_pipeline_does_not_exit() {
    let r = run("pipe-exit", "exit | cat\necho still here\n");
    assert_eq!(r.stdout, "still here\n");
}

#[test]
fn histfile_is_loaded_and_saved() {
    let dir = temp_dir("histfile");
    let histfile = dir.join("hist");
    fs::write(&histfile, "seed one\nseed two\n").unwrap();

    let r = run_in(&dir, "history\n", Some(&histfile));
    assert_eq!(
        r.stdout,
        "    1  seed one\n    2  seed two\n    3  history\n"
    );
    assert_eq!(
        fs::read_to_string(&histfile).unwrap(),
        "seed one\nseed two\nhistory\n"
    );
}

#[test]
fn background_job_is_listed_then_reported_done() {
    // `sleep 0.2` is still running when `jobs` runs straight away, and has
    // finished by the time the foreground `sleep 0.5` returns.
    let r = run("jobs", "sleep 0.2 &\njobs\nsleep 0.5\n");
    let lines: Vec<&str> = r.stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{:?}", r.stdout);

    let pid = lines[0].strip_prefix("[1] ").expect(lines[0]);
    assert!(pid.parse::<u32>().is_ok(), "{:?}", lines[0]);
    assert_eq!(lines[1], format!("[1]+  {:<24}sleep 0.2 &", "Running"));
    assert_eq!(lines[2], format!("[1]+  {:<24}sleep 0.2", "Done"));
}

#[test]
fn complete_registration_is_printed_normalized() {
    let r = run(
        "complete-p",
        "complete  -C  /path/to/script   git\ncomplete   -C /other    docker\n\
         complete -p git\ncomplete -p docker\ncomplete -p svn\n",
    );
    assert_eq!(
        r.stdout,
        "complete -C '/path/to/script' git\ncomplete -C '/other' docker\n"
    );
    assert_eq!(r.stderr, "complete: svn: no completion specification\n");
}

#[test]
fn complete_r_unregisters() {
    let r = run(
        "complete-r",
        "complete  -C  /path/to/script  git\ncomplete -r git\ncomplete -p git\n",
    );
    assert_eq!(r.stdout, "");
    assert_eq!(r.stderr, "complete: git: no completion specification\n");
}

#[test]
fn declared_variables_expand_in_arguments() {
    let r = run(
        "expand",
        "declare Variable_1=Value_1\ndeclare Variable_2=Value2\n\
         printf '[%s]' $Variable_1 raw_$Variable_2 '$Variable_1' \"$Variable_2\"\necho\n",
    );
    assert_eq!(r.stdout, "[Value_1][raw_Value2][$Variable_1][Value2]\n");
}

#[test]
fn environment_variables_expand_too() {
    let r = run("expand-env", "echo $HOME_SET_BY_TEST\n");
    assert_eq!(r.stdout, "\n");
    let dir = temp_dir("expand-env-home");
    let r = run_in(&dir, "echo $HOME\n", None);
    assert_eq!(r.stdout, format!("{}\n", dir.display()));
}

#[test]
fn empty_expansions_drop_unquoted_words_only() {
    let r = run(
        "expand-empty",
        "declare existing=value\n\
         printf '[%s]' ${missing1}end ${existing} ${missing2} $missing3 \"$missing4\"\necho\n\
         \"\"\n",
    );
    assert_eq!(r.stdout, "[end][value][]\n");
    assert_eq!(r.stderr, ": not found\n");
}

#[test]
fn cd_tilde_slash_is_relative_to_home() {
    let dir = temp_dir("cd-tilde");
    fs::create_dir(dir.join("sub")).unwrap();
    let r = run_in(&dir, "cd /\ncd ~/sub\npwd\n", None);
    let real = dir.canonicalize().unwrap();
    assert_eq!(r.stdout, format!("{}\n", real.join("sub").display()));
    assert_eq!(r.stderr, "");
}

#[test]
fn quoted_pipe_is_not_a_pipeline() {
    let r = run("quoted-pipe", "echo \"a|b\" 'c | d' e\\|f\n");
    assert_eq!(r.stdout, "a|b c | d e|f\n");
    assert_eq!(r.stderr, "");
}

#[test]
fn quoted_redirect_and_ampersand_are_words() {
    let dir = temp_dir("quoted-redirect");
    let r = run_in(&dir, "echo \">\" x \"&\"\n", None);
    assert_eq!(r.stdout, "> x &\n");
    assert!(!dir.join("x").exists());
}

#[test]
fn redirect_without_spaces() {
    let r = run(
        "redir-nospace",
        "echo hi>f\nls /no_such_path_xyz 2>>e\ncat f e\n",
    );
    assert!(r.stdout.starts_with("hi\nls: "), "{:?}", r.stdout);
    assert_eq!(r.stderr, "");
}

#[test]
fn failed_redirect_is_reported_and_command_skipped() {
    let r = run(
        "redir-fail",
        "echo hi > /no/such/dir/f\necho err 2>> /no/such/dir/e\necho after\n",
    );
    assert_eq!(r.stdout, "after\n");
    assert_eq!(
        r.stderr,
        "/no/such/dir/f: No such file or directory\n/no/such/dir/e: No such file or directory\n"
    );
}

#[test]
fn failed_redirect_skips_only_its_pipeline_stage() {
    let r = run(
        "redir-fail-pipe",
        "echo a > /no/such/f | cat\necho b | cat > /no/such/g\necho c\n",
    );
    assert_eq!(r.stdout, "c\n");
    assert_eq!(
        r.stderr,
        "/no/such/f: No such file or directory\n/no/such/g: No such file or directory\n"
    );
}
