//! Online metadata provider backed by the crates.io API.
//!
//! Requires the `online` Cargo feature.

use super::{CrateMetadata, MetadataProvider};
use crate::analysis::types::Dependency;
use serde::Deserialize;
use tracing::debug;

const CRATES_IO_API: &str = "https://crates.io/api/v1/crates";

/// HTTP transport used by [`CratesIoProvider`].
trait HttpClient: Send + Sync {
    /// Fetch the body of `url` as a string.
    fn get(&self, url: &str) -> Result<String, String>;
}

struct UreqClient {
    agent: ureq::Agent,
}

impl UreqClient {
    fn new() -> Self {
        Self {
            agent: ureq::Agent::new(),
        }
    }
}

impl HttpClient for UreqClient {
    fn get(&self, url: &str) -> Result<String, String> {
        self.agent
            .get(url)
            .set("User-Agent", concat!("amber/", env!("CARGO_PKG_VERSION")))
            .call()
            .map_err(|e| e.to_string())?
            .into_string()
            .map_err(|e| e.to_string())
    }
}

/// Online metadata provider using crates.io.
pub struct CratesIoProvider {
    client: Box<dyn HttpClient>,
    base_url: String,
}

impl CratesIoProvider {
    /// Create a new online provider.
    #[must_use]
    pub fn new() -> Self {
        Self::with_client(Box::new(UreqClient::new()), CRATES_IO_API)
    }

    fn with_client(client: Box<dyn HttpClient>, base_url: &str) -> Self {
        Self {
            client,
            base_url: base_url.to_string(),
        }
    }

    fn fetch_crate(&self, name: &str) -> crate::amber_anyhow::Result<CrateResponse> {
        let url = format!("{}/{name}", self.base_url);
        debug!("Fetching crates.io metadata for {}", name);

        let body = self
            .client
            .get(&url)
            .map_err(|e| crate::anyhow!("failed to fetch crates.io metadata for {name}: {e}"))?;

        serde_json::from_str::<CrateResponse>(&body)
            .map_err(|e| crate::anyhow!("failed to parse crates.io response for {name}: {e}"))
    }
}

impl Default for CratesIoProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataProvider for CratesIoProvider {
    fn fetch(&self, dep: &Dependency) -> crate::amber_anyhow::Result<CrateMetadata> {
        let data = self.fetch_crate(&dep.name)?;

        let info = data.crate_data;
        let versions = data.versions.unwrap_or_default();

        let last_release = versions
            .iter()
            .max_by_key(|v| &v.created_at)
            .map(|v| v.created_at.clone());

        let downloads = info.downloads.unwrap_or(0);

        // Simple maintenance heuristic: higher downloads and recent releases score better.
        let maintenance_score = if downloads > 10_000_000 {
            95
        } else if downloads > 1_000_000 {
            85
        } else if downloads > 100_000 {
            75
        } else if downloads > 10_000 {
            65
        } else {
            50
        };

        Ok(CrateMetadata {
            download_count: downloads,
            last_release,
            maintenance_score,
            cve_count: 0,
            yanked: false,
            deprecated: false,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CrateResponse {
    #[serde(rename = "crate")]
    crate_data: CrateInfo,
    versions: Option<Vec<VersionInfo>>,
}

#[derive(Debug, Deserialize)]
struct CrateInfo {
    downloads: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct VersionInfo {
    created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::types::{Dependency, DependencyKind, DependencySource};

    struct MockClient {
        response: Result<String, String>,
    }

    impl HttpClient for MockClient {
        fn get(&self, _url: &str) -> Result<String, String> {
            self.response.clone()
        }
    }

    fn dummy_dep() -> Dependency {
        Dependency {
            name: "serde".to_string(),
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
    fn successful_fetch_populates_metadata() {
        let body = r#"{
            "crate": {"downloads": 50000},
            "versions": [
                {"created_at": "2023-01-01T00:00:00Z"},
                {"created_at": "2024-01-01T00:00:00Z"}
            ]
        }"#
        .to_string();
        let provider = CratesIoProvider::with_client(
            Box::new(MockClient { response: Ok(body) }),
            "https://crates.io/api/v1/crates",
        );

        let meta = provider.fetch(&dummy_dep()).unwrap();
        assert_eq!(meta.download_count, 50_000);
        assert_eq!(meta.maintenance_score, 65);
        assert_eq!(meta.last_release, Some("2024-01-01T00:00:00Z".to_string()));
    }

    #[test]
    fn http_error_returns_error() {
        let provider = CratesIoProvider::with_client(
            Box::new(MockClient {
                response: Err("network unreachable".to_string()),
            }),
            "https://crates.io/api/v1/crates",
        );

        assert!(provider.fetch(&dummy_dep()).is_err());
    }

    #[test]
    fn invalid_json_returns_error() {
        let provider = CratesIoProvider::with_client(
            Box::new(MockClient {
                response: Ok("not json".to_string()),
            }),
            "https://crates.io/api/v1/crates",
        );

        assert!(provider.fetch(&dummy_dep()).is_err());
    }

    #[test]
    fn missing_versions_defaults_empty() {
        let body = r#"{"crate": {"downloads": 1000}}"#.to_string();
        let provider = CratesIoProvider::with_client(
            Box::new(MockClient { response: Ok(body) }),
            "https://crates.io/api/v1/crates",
        );

        let meta = provider.fetch(&dummy_dep()).unwrap();
        assert_eq!(meta.download_count, 1_000);
        assert!(meta.last_release.is_none());
    }

    #[test]
    #[allow(clippy::default_constructed_unit_structs)]
    fn default_provider_matches_new() {
        // Default delegates to new(); construction must not panic.
        let _ = CratesIoProvider::default();
        let _ = CratesIoProvider::new();
    }
}
