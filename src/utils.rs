use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;

pub fn is_executable(path: &std::path::Path) -> bool {
    if let Ok(metadata) = fs::metadata(path) {
        return metadata.permissions().mode() & 0o111 != 0;
    }
    false
}

pub fn find_in_path(command: &str) -> Option<String> {
    let path_os = env::var_os("PATH")?;
    for dir in env::split_paths(&path_os) {
        let candidate = dir.join(command);
        if candidate.exists() && is_executable(&candidate) {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}
