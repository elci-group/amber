//! amber_regex — Replacement for the `regex` crate
//!
//! For simple patterns, prefer `str::split`, `str::contains`, or a hand-rolled
//! state machine. For full regex support, consider `regex-lite` or isolate
/// the regex engine behind a small internal trait.

/// A minimal regex-like matcher for literal substrings.
#[derive(Debug, Clone)]
pub struct SimpleMatcher {
    pattern: String,
}

impl SimpleMatcher {
    /// Create a new literal matcher.
    #[must_use]
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
        }
    }

    /// Returns true when `haystack` contains the literal pattern.
    #[must_use]
    pub fn is_match(&self, haystack: &str) -> bool {
        haystack.contains(&self.pattern)
    }
}

