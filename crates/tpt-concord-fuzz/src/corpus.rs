// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright 2026 TPT Solutions

//! Directory-backed corpus management.
//!
//! Corpus entries are content-addressed JSON files — the same input is
//! stored once, and the corpus can seed campaigns deterministically.

use crate::input_digest;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value as Json;
use std::path::{Path, PathBuf};

/// A corpus of serialized inputs backed by `dir/corpus/`.
#[derive(Debug, Clone)]
pub struct Corpus {
    dir: PathBuf,
}

impl Corpus {
    /// Open (creating if needed) the corpus under `dir`.
    pub fn open(dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(dir.join("corpus"))?;
        Ok(Self { dir })
    }

    /// Add an input; returns true if it was new to the corpus.
    pub fn add<I: Serialize>(&self, input: &I) -> std::io::Result<bool> {
        let json = serde_json::to_value(input)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let name = format!("{}.json", input_digest(&json));
        let path = self.dir.join("corpus").join(name);
        if path.exists() {
            return Ok(false);
        }
        std::fs::write(path, json.to_string())?;
        Ok(true)
    }

    /// All stored inputs, in content-hash order (deterministic).
    pub fn entries<I: DeserializeOwned>(&self) -> Vec<I> {
        let mut names: Vec<PathBuf> = self
            .file_names()
            .map(|p| self.dir.join("corpus").join(p))
            .collect();
        names.sort();
        names
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .filter_map(|s| serde_json::from_str(&s).ok())
            .collect()
    }

    /// Raw JSON entries (works for inputs whose type isn't known).
    pub fn entries_json(&self) -> Vec<Json> {
        let mut names: Vec<PathBuf> = self
            .file_names()
            .map(|p| self.dir.join("corpus").join(p))
            .collect();
        names.sort();
        names
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .filter_map(|s| serde_json::from_str(&s).ok())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.file_names().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn file_names(&self) -> impl Iterator<Item = String> {
        std::fs::read_dir(self.dir.join("corpus"))
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.ends_with(".json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_corpus(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("concord-corpus-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn add_dedupes_and_entries_are_deterministic() {
        let dir = tmp_corpus("dedupe");
        let corpus = Corpus::open(&dir).unwrap();
        assert!(corpus.is_empty());

        assert!(corpus.add(&vec![1i64, 2, 3]).unwrap(), "first add is new");
        assert!(!corpus.add(&vec![1, 2, 3]).unwrap(), "same input dedupes");
        assert!(corpus.add(&vec![9]).unwrap());
        assert_eq!(corpus.len(), 2);

        let entries: Vec<Vec<i64>> = corpus.entries();
        assert_eq!(entries.len(), 2);
        assert!(entries.contains(&vec![1, 2, 3]));
        assert!(entries.contains(&vec![9]));
        // Hash order is stable: the same corpus always yields the same order.
        let entries_again: Vec<Vec<i64>> = corpus.entries();
        assert_eq!(entries, entries_again);

        // Reopening sees the same corpus.
        let reopened = Corpus::open(&dir).unwrap();
        assert_eq!(reopened.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn raw_json_entries_work_without_a_type() {
        let dir = tmp_corpus("raw");
        let corpus = Corpus::open(&dir).unwrap();
        corpus.add(&serde_json::json!({"op": "push"})).unwrap();
        let raw = corpus.entries_json();
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0]["op"], "push");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
