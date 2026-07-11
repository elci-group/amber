//! Persistent library of replacement modules backed by Padagonia.

use padagonia::{KeyId, Node, Provenance, Scalar, Store, StringTableExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const LABEL_REPLACEMENT: &str = "ReplacementModule";
const PROP_CRATE: &str = "crate_name";
const PROP_MODULE: &str = "module_name";
const PROP_CODE: &str = "code";
const PROP_SOURCE: &str = "source";
const PROP_CREATED: &str = "created_at";

/// Origin of a library entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntrySource {
    /// Generated in-house by amber.
    Generated,
    /// Imported from an external source.
    Imported,
    /// Forked from an existing library entry.
    Forked,
}

impl EntrySource {
    /// String form used for storage.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Imported => "imported",
            Self::Forked => "forked",
        }
    }

    /// Parse a stored string back into a source variant.
    #[must_use]
    pub fn from_stored(s: &str) -> Self {
        match s {
            "imported" => Self::Imported,
            "forked" => Self::Forked,
            _ => Self::Generated,
        }
    }
}

/// A single replacement module stored in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryEntry {
    /// Crate that this module replaces.
    pub crate_name: String,
    /// Name of the generated module (e.g. `amber_anyhow`).
    pub module_name: String,
    /// Source code of the module.
    pub code: String,
    /// Origin of the entry.
    pub source: EntrySource,
    /// Unix timestamp when the entry was created.
    pub created_at: u64,
}

impl LibraryEntry {
    /// Create a new entry with the current timestamp.
    #[must_use]
    pub fn new(
        crate_name: impl Into<String>,
        module_name: impl Into<String>,
        code: impl Into<String>,
        source: EntrySource,
    ) -> Self {
        Self {
            crate_name: crate_name.into(),
            module_name: module_name.into(),
            code: code.into(),
            source,
            created_at: now_unix(),
        }
    }
}

/// Wrapper around a Padagonia store that holds replacement modules.
#[derive(Debug)]
pub struct LibraryStore {
    store: Store,
    path: PathBuf,
}

impl LibraryStore {
    /// Open an existing library or create an empty one at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if an existing file cannot be read.
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let store = if path.exists() {
            Store::load(&path).map_err(io_err)?
        } else {
            Store::new()
        };
        Ok(Self { store, path })
    }

    /// Persist the library to its path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.store.save(&self.path).map_err(io_err)
    }

    /// Find the most recent entry for `crate_name`.
    #[must_use]
    pub fn find(&self, crate_name: &str) -> Option<LibraryEntry> {
        let label_id = self.store.string_table.label_id(LABEL_REPLACEMENT)?;
        let ids = self.store.node_label_index.get(&label_id)?;
        ids.iter()
            .filter_map(|id| self.store.nodes.get(id))
            .filter_map(|node| self.entry_from_node(node))
            .filter(|entry| entry.crate_name == crate_name)
            .max_by_key(|entry| entry.created_at)
    }

    /// Search entries by crate name, module name, or code contents.
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<LibraryEntry> {
        self.list()
            .into_iter()
            .filter(|e| {
                e.crate_name.contains(query)
                    || e.module_name.contains(query)
                    || e.code.contains(query)
            })
            .collect()
    }

    /// Remove all entries for `crate_name`. Returns `true` if any were removed.
    ///
    /// # Errors
    ///
    /// Returns an error if the library cannot be saved.
    pub fn remove(&mut self, crate_name: &str) -> std::io::Result<bool> {
        let Some(label_id) = self.store.string_table.label_id(LABEL_REPLACEMENT) else {
            return Ok(false);
        };
        let Some(ids) = self.store.node_label_index.get(&label_id).cloned() else {
            return Ok(false);
        };

        let mut removed = false;
        for id in ids {
            if let Some(node) = self.store.nodes.get(&id) {
                if let Some(entry) = self.entry_from_node(node) {
                    if entry.crate_name == crate_name {
                        self.store.nodes.remove(&id);
                        removed = true;
                    }
                }
            }
        }

        if removed {
            self.save()?;
        }
        Ok(removed)
    }

    /// List all entries.
    #[must_use]
    pub fn list(&self) -> Vec<LibraryEntry> {
        let Some(label_id) = self.store.string_table.label_id(LABEL_REPLACEMENT) else {
            return Vec::new();
        };
        let Some(ids) = self.store.node_label_index.get(&label_id) else {
            return Vec::new();
        };
        ids.iter()
            .filter_map(|id| self.store.nodes.get(id))
            .filter_map(|node| self.entry_from_node(node))
            .collect()
    }

    /// Insert an entry and persist.
    ///
    /// # Errors
    ///
    /// Returns an error if the library cannot be saved.
    pub fn insert(&mut self, entry: &LibraryEntry) -> std::io::Result<()> {
        self.insert_node(entry);
        self.save()
    }

    /// Insert many entries without persisting; call [`LibraryStore::save`]
    /// afterwards. Useful for bulk loading or benchmarking.
    pub fn insert_many(&mut self, entries: &[LibraryEntry]) {
        for entry in entries {
            self.insert_node(entry);
        }
    }

    /// Fork an existing entry for `crate_name` with new code and persist.
    ///
    /// # Errors
    ///
    /// Returns an error if no entry exists or the library cannot be saved.
    pub fn fork(&mut self, crate_name: &str, new_code: &str) -> std::io::Result<LibraryEntry> {
        let existing = self.find(crate_name).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no library entry for {crate_name}"),
            )
        })?;
        let forked = LibraryEntry {
            crate_name: existing.crate_name,
            module_name: existing.module_name,
            code: new_code.to_string(),
            source: EntrySource::Forked,
            created_at: now_unix(),
        };
        self.insert_node(&forked);
        self.save()?;
        Ok(forked)
    }

    fn insert_node(&mut self, entry: &LibraryEntry) {
        let props = vec![
            (PROP_CRATE, Scalar::String(entry.crate_name.clone())),
            (PROP_MODULE, Scalar::String(entry.module_name.clone())),
            (PROP_CODE, Scalar::String(entry.code.clone())),
            (
                PROP_SOURCE,
                Scalar::String(entry.source.as_str().to_string()),
            ),
            (PROP_CREATED, Scalar::Timestamp(entry.created_at)),
        ];
        let prov = Provenance::new(
            "amber",
            env!("CARGO_PKG_VERSION"),
            1.0,
            0.0,
            entry.created_at,
            Vec::new(),
        );
        self.store.add_node(LABEL_REPLACEMENT, props, None, prov);
    }

    fn entry_from_node(&self, node: &Node) -> Option<LibraryEntry> {
        let props = &node.properties;
        let crate_name = self.prop_string(props, PROP_CRATE)?;
        let module_name = self.prop_string(props, PROP_MODULE)?;
        let code = self.prop_string(props, PROP_CODE)?;
        let source = self
            .prop_string(props, PROP_SOURCE)
            .map_or(EntrySource::Generated, |s| EntrySource::from_stored(&s));
        let created_at = self.prop_timestamp(props, PROP_CREATED).unwrap_or(0);
        Some(LibraryEntry {
            crate_name,
            module_name,
            code,
            source,
            created_at,
        })
    }

    fn prop_string(&self, props: &[(KeyId, Scalar)], key: &str) -> Option<String> {
        let key_id = self.store.string_table.key_id(key)?;
        props.iter().find(|(k, _)| *k == key_id).and_then(|(_, v)| {
            if let Scalar::String(s) = v {
                Some(s.clone())
            } else {
                None
            }
        })
    }

    fn prop_timestamp(&self, props: &[(KeyId, Scalar)], key: &str) -> Option<u64> {
        let key_id = self.store.string_table.key_id(key)?;
        props.iter().find(|(k, _)| *k == key_id).and_then(|(_, v)| {
            if let Scalar::Timestamp(t) = v {
                Some(*t)
            } else {
                None
            }
        })
    }
}

fn io_err(e: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::{EntrySource, LibraryEntry, LibraryStore};

    #[test]
    fn open_creates_empty_library() {
        let path = temp_path("open_creates_empty_library");
        let lib = LibraryStore::open(&path).unwrap();
        assert!(lib.list().is_empty());
        assert!(lib.find("anyhow").is_none());
    }

    #[test]
    fn insert_and_find_round_trip() {
        let path = temp_path("insert_and_find_round_trip");
        let mut lib = LibraryStore::open(&path).unwrap();
        let entry = LibraryEntry::new(
            "anyhow",
            "amber_anyhow",
            "pub fn anyhow() {}",
            EntrySource::Generated,
        );
        lib.insert(&entry).unwrap();

        let found = lib.find("anyhow").unwrap();
        assert_eq!(found.module_name, "amber_anyhow");
        assert_eq!(found.source, EntrySource::Generated);
    }

    #[test]
    fn list_returns_all_entries() {
        let path = temp_path("list_returns_all_entries");
        let mut lib = LibraryStore::open(&path).unwrap();
        lib.insert(&LibraryEntry::new(
            "anyhow",
            "amber_anyhow",
            "code1",
            EntrySource::Generated,
        ))
        .unwrap();
        lib.insert(&LibraryEntry::new(
            "colored",
            "amber_colored",
            "code2",
            EntrySource::Imported,
        ))
        .unwrap();
        assert_eq!(lib.list().len(), 2);
    }

    #[test]
    fn fork_creates_forked_entry() {
        let path = temp_path("fork_creates_forked_entry");
        let mut lib = LibraryStore::open(&path).unwrap();
        lib.insert(&LibraryEntry::new(
            "anyhow",
            "amber_anyhow",
            "code1",
            EntrySource::Generated,
        ))
        .unwrap();
        let forked = lib.fork("anyhow", "code2").unwrap();
        assert_eq!(forked.source, EntrySource::Forked);
        assert_eq!(forked.code, "code2");
        assert_eq!(lib.list().len(), 2);
    }

    #[test]
    fn search_matches_crate_name() {
        let path = temp_path("search_matches_crate_name");
        let mut lib = LibraryStore::open(&path).unwrap();
        lib.insert(&LibraryEntry::new(
            "anyhow",
            "amber_anyhow",
            "code1",
            EntrySource::Generated,
        ))
        .unwrap();
        lib.insert(&LibraryEntry::new(
            "colored",
            "amber_colored",
            "code2",
            EntrySource::Imported,
        ))
        .unwrap();
        assert_eq!(lib.search("anyhow").len(), 1);
        assert_eq!(lib.search("code2").len(), 1);
        assert_eq!(lib.search("amber_").len(), 2);
        assert!(lib.search("missing").is_empty());
    }

    #[test]
    fn remove_existing_entry_returns_true() {
        let path = temp_path("remove_existing_entry_returns_true");
        let mut lib = LibraryStore::open(&path).unwrap();
        lib.insert(&LibraryEntry::new(
            "anyhow",
            "amber_anyhow",
            "code",
            EntrySource::Generated,
        ))
        .unwrap();
        assert!(lib.remove("anyhow").unwrap());
        assert!(lib.find("anyhow").is_none());
    }

    #[test]
    fn remove_missing_entry_returns_false() {
        let path = temp_path("remove_missing_entry_returns_false");
        let mut lib = LibraryStore::open(&path).unwrap();
        assert!(!lib.remove("anyhow").unwrap());
    }

    #[test]
    fn save_load_round_trip() {
        let path = temp_path("save_load_round_trip");
        {
            let mut lib = LibraryStore::open(&path).unwrap();
            lib.insert(&LibraryEntry::new(
                "anyhow",
                "amber_anyhow",
                "code",
                EntrySource::Generated,
            ))
            .unwrap();
        }
        let lib = LibraryStore::open(&path).unwrap();
        assert!(lib.find("anyhow").is_some());
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "amber-library-test-{}-{name}.pad",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        path
    }
}
