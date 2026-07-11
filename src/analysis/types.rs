use serde::{Deserialize, Serialize};

/// Represents a single dependency with full metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub source: DependencySource,
    pub kind: DependencyKind,
    pub features: Vec<String>,
    pub optional: bool,
    pub uses_default_features: bool,
    /// Transitive dependencies this crate brings in
    pub transitive_deps: Vec<String>,
    /// Approximate lines of code in the crate
    pub loc_approx: usize,
    /// Number of public APIs exported
    pub public_api_count: usize,
    /// Last release date (YYYY-MM-DD)
    pub last_release: Option<String>,
    /// Maintenance status score (0-100)
    pub maintenance_score: u8,
    /// Known CVE count
    pub cve_count: usize,
    /// License SPDX identifier
    pub license: Option<String>,
    /// Download count from crates.io
    pub download_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencySource {
    CratesIo,
    Git { url: String, rev: Option<String> },
    Path { path: String },
    Registry(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencyKind {
    Normal,
    Dev,
    Build,
    #[serde(rename = "dev-only")]
    DevOnly,
}

/// Usage statistics for a specific crate within the project
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrateUsage {
    pub crate_name: String,
    /// Specific items imported (functions, types, macros)
    pub imported_items: Vec<ImportedItem>,
    /// Total import statements referencing this crate
    pub import_count: usize,
    /// Call sites where this crate's APIs are used
    pub call_sites: Vec<CallSite>,
    /// Files that import this crate
    pub affected_files: Vec<String>,
    /// Number of unique functions/methods called
    pub unique_api_usage: usize,
    /// Estimated coverage percentage of the crate's API
    pub api_coverage_percent: f64,
    /// Whether the crate is used in public APIs of the project
    pub used_in_public_api: bool,
    /// Feature flags from this crate that are used
    pub used_features: Vec<String>,
    /// Whether the usage is trivial (few calls, simple patterns)
    pub is_trivial_usage: bool,
}

impl CrateUsage {
    /// Whether any source usage of the crate was detected, either through
    /// `use` imports or fully-qualified call sites (e.g. `toml::from_str`).
    #[must_use]
    pub fn has_usage(&self) -> bool {
        !self.imported_items.is_empty() || !self.call_sites.is_empty()
    }
}

/// A source location for a usage event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

impl Location {
    pub fn new(file: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            file: file.into(),
            line,
            column,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedItem {
    pub name: String,
    pub kind: ItemKind,
    pub path: String,
    pub location: Location,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ItemKind {
    Function,
    Type,
    Macro,
    Trait,
    Module,
    Constant,
    Static,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallSite {
    pub function_name: String,
    pub kind: UsageKind,
    pub location: Location,
    pub context: String,
}

/// The kind of API usage detected in source code
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UsageKind {
    Import,
    FunctionCall,
    MethodCall,
    MacroInvocation,
    TypeReference,
    TraitBound,
    Attribute,
    ConstantReference,
}

impl Dependency {
    /// Calculate total transitive dependency count
    #[must_use]
    pub fn total_transitive_count(&self) -> usize {
        self.transitive_deps.len()
    }

    /// Check if this is a high-risk dependency (crypto, TLS, etc.)
    #[must_use]
    pub fn is_security_sensitive(&self) -> bool {
        let sensitive_keywords = [
            "crypto",
            "tls",
            "ssl",
            "rand",
            "hash",
            "digest",
            "signature",
            "cipher",
            "aes",
            "rsa",
            "ecdsa",
            "oauth",
            "jwt",
            "auth",
            "password",
            "bcrypt",
            "serde",
            "serialize",
            "parser",
            "codec",
        ];
        let name_lower = self.name.to_lowercase();
        sensitive_keywords.iter().any(|k| name_lower.contains(k))
    }
}
