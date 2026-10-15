// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! `concord watch` — re-run a concord command whenever its inputs change.
//!
//! Polls modification times of the given files and directories (directories
//! are scanned recursively) and re-runs the command in a child process, so
//! the watched command gets exactly the behaviour and exit codes it has on
//! the command line. Polling uses only the standard library: no platform file
//! watcher, and it behaves the same on Linux, macOS and Windows.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

type Snapshot = BTreeMap<PathBuf, SystemTime>;

/// Record the mtime of every file under `paths`. Missing paths are simply
/// absent, so deleting a watched file also counts as a change.
fn snapshot(paths: &[PathBuf]) -> Snapshot {
    let mut out = Snapshot::new();
    for path in paths {
        collect(path, &mut out);
    }
    out
}

fn collect(path: &Path, out: &mut Snapshot) {
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.is_dir() {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                collect(&entry.path(), out);
            }
        }
    } else if let Ok(modified) = meta.modified() {
        out.insert(path.to_path_buf(), modified);
    }
}

/// Run `concord <command>` once, then again after each change to `paths`.
///
/// `max_runs` stops after that many runs (used by tests and CI smoke checks);
/// `None` watches until interrupted. Returns the exit code of the last run.
pub fn watch(
    paths: &[PathBuf],
    command: &[String],
    interval: Duration,
    max_runs: Option<usize>,
) -> i32 {
    if command.first().map(String::as_str) == Some("watch") {
        eprintln!("watch: refusing to watch itself");
        return 2;
    }
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            eprintln!("watch: cannot locate concord binary: {e}");
            return 2;
        }
    };
    let mut runs = 0usize;
    loop {
        runs += 1;
        eprintln!("--- run {runs}: concord {}", command.join(" "));
        let code = match Command::new(&exe).args(command).status() {
            Ok(status) => status.code().unwrap_or(2),
            Err(e) => {
                eprintln!("watch: cannot run command: {e}");
                return 2;
            }
        };
        eprintln!(
            "--- exit {code}; waiting for changes to {} path(s)",
            paths.len()
        );
        if max_runs.is_some_and(|max| runs >= max) {
            return code;
        }
        let before = snapshot(paths);
        while snapshot(paths) == before {
            sleep(interval);
        }
        eprintln!("--- change detected");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_sees_creation_modification_and_deletion() {
        let dir = std::env::temp_dir().join(format!("concord-watch-unit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        let file = dir.join("nested").join("a.json");

        let empty = snapshot(std::slice::from_ref(&dir));
        std::fs::write(&file, "1").unwrap();
        let created = snapshot(std::slice::from_ref(&dir));
        assert_ne!(empty, created, "creation is a change");

        std::fs::remove_file(&file).unwrap();
        assert_eq!(
            snapshot(std::slice::from_ref(&dir)),
            empty,
            "deletion is a change back"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
