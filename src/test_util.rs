use std::path::PathBuf;
use std::{env, fs, process};

/// A fresh, empty directory unique to this test process and `name`.
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("rust-shell-unit-{}-{}", process::id(), name));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}
