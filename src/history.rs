use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

/// Command history, plus how much of it has already been written to disk
/// (so `history -a` only appends new entries).
#[derive(Default)]
pub struct History {
    entries: Vec<String>,
    synced: usize,
}

impl History {
    pub fn push(&mut self, entry: &str) {
        self.entries.push(entry.to_string());
    }

    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Treat everything recorded so far as already saved.
    pub fn mark_synced(&mut self) {
        self.synced = self.entries.len();
    }

    /// Appends the non-blank lines of `path` to the history.
    pub fn read_from(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let content = fs::read_to_string(path)?;
        self.entries.extend(
            content
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(String::from),
        );
        Ok(())
    }

    /// Overwrites `path` with the full history.
    pub fn write_to(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let file = File::create(path)?;
        write_lines(file, &self.entries)?;
        self.mark_synced();
        Ok(())
    }

    /// Appends the entries added since the last sync to `path`.
    pub fn append_to(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        write_lines(file, &self.entries[self.synced..])?;
        self.mark_synced();
        Ok(())
    }

    /// Prints the last `count` entries (all of them if `None`), numbered.
    pub fn print(&self, count: Option<usize>, out: &mut dyn Write) -> io::Result<()> {
        let start = count.map_or(0, |n| self.entries.len().saturating_sub(n));
        for (i, entry) in self.entries.iter().enumerate().skip(start) {
            writeln!(out, "{:>5}  {}", i + 1, entry)?;
        }
        Ok(())
    }
}

fn write_lines(mut file: File, lines: &[String]) -> io::Result<()> {
    for line in lines {
        writeln!(file, "{}", line)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_dir;

    fn history_of(entries: &[&str]) -> History {
        let mut history = History::default();
        for entry in entries {
            history.push(entry);
        }
        history
    }

    fn printed(history: &History, count: Option<usize>) -> String {
        let mut out = Vec::new();
        history.print(count, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn print_numbers_all_entries() {
        let history = history_of(&["a", "b"]);
        assert_eq!(printed(&history, None), "    1  a\n    2  b\n");
    }

    #[test]
    fn print_limits_to_last_n_keeping_numbers() {
        let history = history_of(&["a", "b", "c"]);
        assert_eq!(printed(&history, Some(2)), "    2  b\n    3  c\n");
        assert_eq!(printed(&history, Some(0)), "");
    }

    #[test]
    fn print_with_n_beyond_length_prints_everything() {
        let history = history_of(&["a", "b"]);
        assert_eq!(printed(&history, Some(10)), printed(&history, None));
    }

    #[test]
    fn read_skips_blank_lines_and_trims() {
        let dir = temp_dir("hist-read");
        let path = dir.join("h");
        fs::write(&path, "one\n\n   \n  two  \n").unwrap();

        let mut history = history_of(&["zero"]);
        history.read_from(&path).unwrap();
        assert_eq!(history.entries(), ["zero", "one", "two"]);
    }

    #[test]
    fn read_of_missing_file_is_an_error() {
        let dir = temp_dir("hist-missing");
        let mut history = History::default();
        assert!(history.read_from(dir.join("nope")).is_err());
        assert!(history.entries().is_empty());
    }

    #[test]
    fn write_overwrites_with_everything() {
        let dir = temp_dir("hist-write");
        let path = dir.join("h");
        fs::write(&path, "stale\n").unwrap();

        let mut history = history_of(&["a", "b"]);
        history.write_to(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "a\nb\n");
    }

    #[test]
    fn append_writes_only_entries_since_last_sync() {
        let dir = temp_dir("hist-append");
        let path = dir.join("h");

        let mut history = history_of(&["a", "b"]);
        history.append_to(&path).unwrap();
        history.push("c");
        history.append_to(&path).unwrap();
        // Nothing new: appending again must not duplicate anything.
        history.append_to(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "a\nb\nc\n");
    }

    #[test]
    fn write_and_mark_synced_reset_the_append_point() {
        let dir = temp_dir("hist-sync");
        let written = dir.join("w");
        let appended = dir.join("a");

        let mut history = history_of(&["loaded"]);
        history.mark_synced();
        history.push("x");
        history.write_to(&written).unwrap();
        history.push("y");
        history.append_to(&appended).unwrap();
        assert_eq!(fs::read_to_string(&appended).unwrap(), "y\n");
    }
}
