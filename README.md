# Rust Shell

[![Tests](https://github.com/gabfec/rust-shell/actions/workflows/rust.yml/badge.svg)](https://github.com/gabfec/rust-shell/actions)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Made with Rust](https://img.shields.io/badge/Made%20with-Rust-red.svg)]()
[![Rust](https://img.shields.io/badge/rust-1.91+-orange.svg)](https://www.rust-lang.org)

A POSIX-style interactive shell written in Rust, with its own **lexer, parser
and executor** implemented from scratch.

Only line editing comes from a library ([`rustyline`](https://crates.io/crates/rustyline)).
Everything else is built in:

- Lexer with single/double quotes, backslash escapes and typed operators
- Parser producing a pipeline of commands with their redirections
- Executor for external programs, builtins and pipelines mixing both
- Shell variables and `$VAR` / `${VAR}` expansion
- Background jobs with job control output (`jobs`, `Done` reports)
- Command history, persisted across sessions
- Tab completion for commands, file names and registered completer scripts

---

## Features

### Builtins

| Builtin | Description |
|---------|-------------|
`echo <word>...` | print arguments
`type <name>` | say whether a name is a builtin or which executable it runs
`pwd` | print the current directory
`cd [<dir>]` | change directory (`~` and no argument mean `$HOME`)
`exit` | leave the shell
`history [<n>]` | list the last `n` commands
`history -r\|-w\|-a <file>` | read, write or append the history file
`jobs` | list background jobs
`declare <name>=<value>` | set a shell variable
`declare -p [<name>]` | print variables
`complete -C <script> <cmd>` | register a completer script for a command
`complete -p\|-r [<cmd>]` | print or remove completion registrations

---

### Command-line syntax

| Feature | Example |
|---------|---------|
Single quotes | `echo 'a  $b'`
Double quotes | `echo "it's $USER"`
Backslash escapes | `echo a\ b \$HOME`
Variables | `echo $HOME ${name}_suffix`
Pipelines | `ls \| grep rs \| wc -l`
Redirect stdout | `cmd > file`, `cmd 1>> file`
Redirect stderr | `cmd 2> file`, `cmd 2>> file`
Background job | `sleep 10 &`

Unquoted words that expand to nothing are dropped, quoted ones are kept as
empty arguments, and operators inside quotes are plain text, as in bash.

---

### Line editing and completion

| Key | Action |
|-----|--------|
`<TAB>` | complete a command, file name or directory
`<TAB><TAB>` | list the candidates when the completion is ambiguous
`<UP>` / `<DOWN>` | browse history
`Ctrl-C` | discard the current line
`Ctrl-D` | exit

When a completer is registered with `complete -C`, it is run with bash's
arguments (command, current word, previous word) and `COMP_LINE` /
`COMP_POINT`, and each line it prints becomes a candidate.

Setting `HISTFILE` loads the history on startup and saves it on exit.

---

## Example usage

Start the shell:

```bash
./your_program.sh
```

Then, for example:

```console
$ declare name=world
$ echo "hello $name" | tr a-z A-Z > greeting.txt
$ cat greeting.txt
HELLO WORLD
$ sleep 2 &
[1] 4242
$ jobs
[1]+  Running                 sleep 2 &
```

## Tests

```bash
cargo test
```

Unit tests sit next to the code they cover (one file per builtin under
`src/builtins/`), and `tests/cli.rs` runs the built shell end to end.

## Project layout

| Path | Contents |
|------|----------|
`src/main.rs` | the read-eval loop
`src/parser.rs` | lexer, variable expansion and parser
`src/executor.rs` | runs pipelines, redirections and background jobs
`src/builtins/` | one file per builtin, plus the builtin table
`src/completion.rs` | tab completion
`src/history.rs` | history and its file
`src/jobs.rs` | the background job table
`src/shell.rs` | state shared across command lines

## Educational Purpose

This project was built as part of the CodeCrafters
["Build Your Own Shell"](https://app.codecrafters.io/courses/shell/overview)
challenge, which focuses on implementing real-world systems from scratch.

<a href="https://app.codecrafters.io/users/codecrafters-bot?r=2qF"><img src="https://backend.codecrafters.io/progress/shell/0232f406-b480-40d1-ae9f-1ac99ec8911f" alt="CodeCrafters progress" width="400"></a>

The goal was to understand how a shell works internally, including tokenizing
and quoting, parsing pipelines, spawning processes and wiring their file
descriptors, job control, and interactive completion.

https://codecrafters.io

## License

MIT License — see LICENSE file for details.
