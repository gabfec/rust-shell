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
