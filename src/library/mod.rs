//! Padagonia-backed library of generated replacement modules.
//!
//! Enabled by the `library` Cargo feature. Stores, retrieves, and forks
//! replacement modules so that they can be reused instead of regenerating
//! new in-house alternatives for the same crate.

pub mod store;

pub use store::{EntrySource, LibraryEntry, LibraryStore};
