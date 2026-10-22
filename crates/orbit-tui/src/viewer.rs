//! Explicit, user-approved external viewer handoff for exported attachments.
//!
//! Orbit never executes attachment content. Opening a file only spawns a
//! well-known viewer binary with the exported path as a single argument; no
//! shell is involved and no attachment bytes are interpreted by Orbit.
use std::path::Path;
use std::process::Command;

/// Candidate viewers, most explicit first. `ORBIT_VIEWER` wins when set.
fn candidates() -> Vec<Vec<String>> {
    let mut candidates = Vec::new();
    if let Ok(configured) = std::env::var("ORBIT_VIEWER")
        && !configured.trim().is_empty()
    {
        let parts: Vec<String> = configured.split_whitespace().map(str::to_owned).collect();
        if !parts.is_empty() {
            candidates.push(parts);
        }
    }
    if cfg!(target_os = "macos") {
        candidates.push(vec!["open".to_owned()]);
    } else if cfg!(target_os = "windows") {
        candidates.push(vec!["explorer".to_owned()]);
    } else {
        candidates.push(vec!["xdg-open".to_owned()]);
    }
    candidates
}

/// Open `path` with the first available viewer. Returns the program used, or a
/// human-readable reason when no viewer is available.
pub fn open_with_viewer(path: &Path) -> Result<String, String> {
    let mut last_error = "no viewer configured".to_owned();
    for candidate in candidates() {
        let (program, arguments) = match candidate.split_first() {
            Some((program, arguments)) => (program, arguments),
            None => continue,
        };
        let mut command = Command::new(program);
        command.args(arguments);
        command.arg(path);
        match command.spawn() {
            Ok(_) => return Ok(program.clone()),
            Err(error) => last_error = format!("{program}: {error}"),
        }
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_candidates_never_use_a_shell() {
        for candidate in candidates() {
            assert!(!candidate.iter().any(|part| part == "-c"));
            assert!(!candidate.iter().any(|part| part.contains("sh")));
        }
    }

    #[test]
    fn configured_viewer_is_preferred() {
        // SAFETY: tests run single-threaded in this module's process.
        unsafe { std::env::set_var("ORBIT_VIEWER", "true") };
        let candidates = candidates();
        assert_eq!(candidates[0], vec!["true".to_owned()]);
        unsafe { std::env::remove_var("ORBIT_VIEWER") };
    }
}
