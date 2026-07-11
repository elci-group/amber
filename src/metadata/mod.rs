//! Dependency metadata providers.
//!
//! By default Amber operates offline and uses conservative estimates.
//! Enable the `online` feature to fetch live data from crates.io.

pub mod offline;
#[cfg(feature = "online")]
pub mod online;
pub mod rustsec;

use crate::analysis::types::Dependency;

/// Metadata that can influence replaceability scoring.
#[derive(Debug, Clone, Default)]
pub struct CrateMetadata {
    pub download_count: u64,
    pub last_release: Option<String>,
    pub maintenance_score: u8,
    pub cve_count: usize,
    pub yanked: bool,
    pub deprecated: bool,
}

/// A source of crate metadata.
pub trait MetadataProvider: Send + Sync {
    /// Fetch metadata for a single crate.
    ///
    /// # Errors
    ///
    /// Returns an error if metadata cannot be fetched or parsed. Callers may
    /// choose to fall back to default metadata when this happens.
    fn fetch(&self, dep: &Dependency) -> crate::amber_anyhow::Result<CrateMetadata>;

    /// Fetch metadata for multiple crates in batch when possible.
    fn fetch_batch(&self, deps: &[Dependency]) -> Vec<crate::amber_anyhow::Result<CrateMetadata>> {
        deps.iter().map(|d| self.fetch(d)).collect()
    }
}

/// A metadata provider that never returns real data.
pub struct NullProvider;

impl MetadataProvider for NullProvider {
    fn fetch(&self, _dep: &Dependency) -> crate::amber_anyhow::Result<CrateMetadata> {
        Ok(CrateMetadata::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::types::{Dependency, DependencyKind, DependencySource};

    fn dummy_dep() -> Dependency {
        Dependency {
            name: "sample".to_string(),
            version: "1.0.0".to_string(),
            source: DependencySource::CratesIo,
            kind: DependencyKind::Normal,
            features: Vec::new(),
            optional: false,
            uses_default_features: true,
            transitive_deps: Vec::new(),
            loc_approx: 0,
            public_api_count: 0,
            last_release: None,
            maintenance_score: 50,
            cve_count: 0,
            license: None,
            download_count: 0,
        }
    }

    #[test]
    fn null_provider_returns_default_metadata() {
        let provider = NullProvider;
        let meta = provider.fetch(&dummy_dep()).unwrap();
        assert_eq!(meta.maintenance_score, 0);
        assert_eq!(meta.cve_count, 0);
    }

    #[test]
    fn fetch_batch_uses_default_implementation() {
        let provider = NullProvider;
        let batch = provider.fetch_batch(&[dummy_dep(), dummy_dep()]);
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].as_ref().unwrap().cve_count, 0);
    }
}
