//! Offline metadata provider.
//!
//! Uses only information already present in Cargo metadata. All scores are
//! neutral defaults so that analysis remains useful without network access.

use super::{CrateMetadata, MetadataProvider};
use crate::analysis::types::Dependency;

/// Offline metadata provider.
pub struct OfflineProvider;

impl OfflineProvider {
    /// Create a new offline provider.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for OfflineProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataProvider for OfflineProvider {
    fn fetch(&self, _dep: &Dependency) -> crate::amber_anyhow::Result<CrateMetadata> {
        Ok(CrateMetadata {
            download_count: 0,
            last_release: None,
            maintenance_score: 50,
            cve_count: 0,
            yanked: false,
            deprecated: false,
        })
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
            maintenance_score: 0,
            cve_count: 0,
            license: None,
            download_count: 0,
        }
    }

    #[test]
    fn offline_provider_returns_neutral_metadata() {
        let provider = OfflineProvider::new();
        let meta = provider.fetch(&dummy_dep()).unwrap();
        assert_eq!(meta.maintenance_score, 50);
        assert_eq!(meta.cve_count, 0);
        assert!(!meta.yanked);
        assert!(!meta.deprecated);
    }

    #[test]
    #[allow(clippy::default_constructed_unit_structs)]
    fn default_matches_new() {
        assert_eq!(
            OfflineProvider::default()
                .fetch(&dummy_dep())
                .unwrap()
                .maintenance_score,
            OfflineProvider::new()
                .fetch(&dummy_dep())
                .unwrap()
                .maintenance_score
        );
    }
}
