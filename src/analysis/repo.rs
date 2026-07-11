use crate::amber_anyhow::{Context, Result};
use cargo_metadata::{CargoOpt, Metadata, MetadataCommand, Package, PackageId};
use semver::Version;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use tracing::info;

use super::types::{Dependency, DependencyKind, DependencySource};
use crate::metadata::offline::OfflineProvider;
use crate::metadata::MetadataProvider;

/// Analyzes a Rust repository's dependency structure
pub struct RepositoryAnalyzer {
    metadata: Metadata,
    provider: Box<dyn MetadataProvider>,
}

impl RepositoryAnalyzer {
    /// Create a new analyzer using the default offline metadata provider.
    ///
    /// # Errors
    ///
    /// Returns an error if the manifest path is invalid or Cargo metadata
    /// cannot be loaded.
    pub fn new(manifest_path: &Path) -> Result<Self> {
        Self::with_provider(manifest_path, Box::new(OfflineProvider::new()))
    }

    /// Create a new analyzer with a custom metadata provider.
    ///
    /// # Errors
    ///
    /// Returns an error if the manifest path is invalid or Cargo metadata
    /// cannot be loaded.
    pub fn with_provider(
        manifest_path: &Path,
        provider: Box<dyn MetadataProvider>,
    ) -> Result<Self> {
        info!("Loading Cargo metadata from {}", manifest_path.display());

        let mut cmd = MetadataCommand::new();
        cmd.manifest_path(manifest_path)
            .features(CargoOpt::AllFeatures);

        let metadata = cmd.exec().context(
            "Failed to load Cargo metadata. Is this a valid Rust project with Cargo.toml?",
        )?;

        Ok(Self { metadata, provider })
    }

    /// List all dependencies in the workspace.
    ///
    /// # Errors
    ///
    /// Returns an error if the workspace has no root package.
    pub fn list_dependencies(
        &self,
        include_transitive: bool,
        include_dev: bool,
    ) -> Result<Vec<Dependency>> {
        let root_package = self
            .metadata
            .root_package()
            .context("No root package found in workspace")?;

        let mut deps = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Collect all packages including dependencies
        let all_packages: HashMap<&PackageId, &Package> =
            self.metadata.packages.iter().map(|p| (&p.id, p)).collect();

        // Resolve graph for transitive dependencies
        let resolve = self.metadata.resolve.as_ref();

        for dep in &root_package.dependencies {
            if !include_dev && dep.kind == cargo_metadata::DependencyKind::Development {
                continue;
            }

            // Cargo allows renaming a dependency (`new = { package = "old" }`).
            // The source code uses the declared name, so that is what Amber should
            // display and match against; the original package name is used for
            // metadata lookups and transitive resolution.
            let original_name = dep.name.clone();
            let display_name = dep.rename.clone().unwrap_or_else(|| original_name.clone());
            if !seen.insert(display_name.clone()) {
                continue;
            }

            // Find the resolved package for this dependency
            let dep_package = all_packages.values().find(|p| {
                p.name == original_name
                    && dep.req.matches(
                        &Version::parse(&p.version.to_string()).unwrap_or(Version::new(0, 0, 0)),
                    )
            });

            let transitive_deps = if include_transitive {
                Self::get_transitive_deps(&original_name, resolve, &all_packages)
            } else {
                Vec::new()
            };

            let dependency =
                self.build_dependency_info(dep, &display_name, dep_package, transitive_deps);
            deps.push(dependency);
        }

        // Sort by name for consistent output
        deps.sort_by(|a, b| a.name.cmp(&b.name));

        info!("Found {} direct dependencies", deps.len());
        Ok(deps)
    }

    /// Get a specific dependency by name.
    ///
    /// # Errors
    ///
    /// Returns an error if the dependency is not found.
    pub fn get_dependency(&self, crate_name: &str) -> Result<Dependency> {
        let deps = self.list_dependencies(true, true)?;
        deps.into_iter()
            .find(|d| d.name == crate_name)
            .with_context(|| format!("Dependency '{crate_name}' not found in project"))
    }

    fn get_transitive_deps<'a>(
        crate_name: &str,
        resolve: Option<&cargo_metadata::Resolve>,
        all_packages: &HashMap<&'a PackageId, &'a Package>,
    ) -> Vec<String> {
        let mut transitive = Vec::new();

        let Some(resolve) = resolve else {
            transitive.sort();
            transitive.dedup();
            return transitive;
        };

        // Find the starting node by package name, then walk the full resolve graph
        // to collect the transitive closure. This includes all dependency kinds
        // (normal, build, dev) that Cargo has resolved.
        let Some(start_id) = resolve.nodes.iter().find_map(|node| {
            all_packages
                .get(&node.id)
                .filter(|pkg| pkg.name == crate_name)
                .map(|_| node.id.clone())
        }) else {
            transitive.sort();
            transitive.dedup();
            return transitive;
        };

        let mut visited = HashSet::<PackageId>::new();
        let mut queue = VecDeque::new();
        visited.insert(start_id.clone());
        queue.push_back(start_id);

        while let Some(id) = queue.pop_front() {
            let Some(node) = resolve.nodes.iter().find(|node| node.id == id) else {
                continue;
            };
            for dep_id in &node.dependencies {
                if let Some(dep_pkg) = all_packages.get(dep_id) {
                    if dep_pkg.name != crate_name && visited.insert(dep_id.clone()) {
                        transitive.push(dep_pkg.name.clone());
                        queue.push_back(dep_id.clone());
                    }
                }
            }
        }

        transitive.sort();
        transitive.dedup();
        transitive
    }

    fn build_dependency_info(
        &self,
        dep: &cargo_metadata::Dependency,
        display_name: &str,
        dep_package: Option<&&Package>,
        transitive_deps: Vec<String>,
    ) -> Dependency {
        let source = dep_package.map_or(DependencySource::CratesIo, |pkg| match &pkg.source {
            Some(src) if src.is_crates_io() => DependencySource::CratesIo,
            Some(src) => DependencySource::Registry(src.to_string()),
            None => DependencySource::Path {
                path: pkg
                    .manifest_path
                    .parent()
                    .map(std::string::ToString::to_string)
                    .unwrap_or_default(),
            },
        });

        let kind = match dep.kind {
            cargo_metadata::DependencyKind::Development => DependencyKind::Dev,
            cargo_metadata::DependencyKind::Build => DependencyKind::Build,
            _ => DependencyKind::Normal,
        };

        let loc_approx = dep_package.map_or(0, |p| Self::estimate_loc(p));
        let public_api_count = dep_package.map_or(0, |p| p.targets.len() * 15);
        let license = dep_package.and_then(|p| {
            p.license
                .as_ref()
                .map(|l| l.split('/').next().unwrap_or(l).trim().to_string())
        });

        let mut dependency = Dependency {
            name: display_name.to_string(),
            version: dep_package.map_or_else(|| dep.req.to_string(), |p| p.version.to_string()),
            source,
            kind,
            features: dep.features.clone(),
            optional: dep.optional,
            uses_default_features: dep.uses_default_features,
            transitive_deps,
            loc_approx,
            public_api_count,
            last_release: None,
            maintenance_score: 50,
            cve_count: 0,
            license,
            download_count: 0,
        };

        match self.provider.fetch(&dependency) {
            Ok(meta) => {
                dependency.last_release = meta.last_release;
                dependency.maintenance_score = meta.maintenance_score;
                dependency.cve_count = meta.cve_count;
                dependency.download_count = meta.download_count;
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to fetch metadata for {}: {e}; using defaults",
                    dependency.name
                );
            }
        }

        dependency
    }

    fn estimate_loc(package: &Package) -> usize {
        // Rough estimate: count source files and estimate
        let src_count = package
            .targets
            .iter()
            .filter(|t| t.kind.iter().any(|k| k == "lib" || k == "bin"))
            .count();
        src_count * 800 // Rough average of 800 LOC per target
    }
}

#[cfg(test)]
impl RepositoryAnalyzer {
    fn from_metadata(metadata: Metadata, provider: Box<dyn MetadataProvider>) -> Self {
        Self { metadata, provider }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_manifest() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("sample_project")
            .join("Cargo.toml")
    }

    fn path_dependency_manifest() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("path_dependency_project")
            .join("app")
            .join("Cargo.toml")
    }

    #[test]
    fn analyzer_loads_sample_project() {
        let analyzer = RepositoryAnalyzer::new(&sample_manifest());
        assert!(analyzer.is_ok());
    }

    #[test]
    fn lists_direct_dependencies() {
        let analyzer = RepositoryAnalyzer::new(&sample_manifest()).unwrap();
        let deps = analyzer.list_dependencies(false, true).unwrap();
        let names: Vec<_> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"anyhow"));
        assert!(names.contains(&"serde"));
        assert!(names.contains(&"serde_json"));
        assert!(names.contains(&"unused_crate"));
    }

    #[test]
    fn excludes_dev_dependencies_when_requested() {
        let analyzer = RepositoryAnalyzer::new(&sample_manifest()).unwrap();
        let deps = analyzer.list_dependencies(false, false).unwrap();
        let names: Vec<_> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"serde"));
        // sample_project has no dev-dependencies, so this mainly checks it does not panic
        assert!(!names.is_empty());
    }

    #[test]
    fn get_dependency_finds_known_crate() {
        let analyzer = RepositoryAnalyzer::new(&sample_manifest()).unwrap();
        let dep = analyzer.get_dependency("anyhow");
        assert!(dep.is_ok());
        assert_eq!(dep.unwrap().name, "anyhow");
    }

    #[test]
    fn get_dependency_missing_crate_errors() {
        let analyzer = RepositoryAnalyzer::new(&sample_manifest()).unwrap();
        let dep = analyzer.get_dependency("definitely_missing_crate");
        assert!(dep.is_err());
    }

    #[test]
    fn analyzer_rejects_invalid_manifest() {
        let bad = PathBuf::from("/definitely/not/a/Cargo.toml");
        assert!(RepositoryAnalyzer::new(&bad).is_err());
    }

    #[test]
    fn path_dependency_is_resolved() {
        let metadata = cargo_metadata::MetadataCommand::new()
            .manifest_path(path_dependency_manifest())
            .exec()
            .unwrap();
        let analyzer =
            RepositoryAnalyzer::from_metadata(metadata, Box::new(OfflineProvider::new()));
        let deps = analyzer.list_dependencies(false, true).unwrap();
        let helper = deps.iter().find(|d| d.name == "helper");
        assert!(helper.is_some(), "expected helper dependency");
        let helper = helper.unwrap();
        assert!(
            matches!(helper.source, DependencySource::Path { .. }),
            "expected path source"
        );
        assert!(helper.loc_approx > 0, "expected loc estimate");
    }

    #[test]
    fn lists_dependencies_with_transitive_flag() {
        let analyzer = RepositoryAnalyzer::new(&sample_manifest()).unwrap();
        let with = analyzer.list_dependencies(true, true).unwrap();
        let without = analyzer.list_dependencies(false, true).unwrap();
        assert_eq!(with.len(), without.len());
    }

    #[test]
    fn full_metadata_resolves_transitive_dependencies() {
        let metadata = cargo_metadata::MetadataCommand::new()
            .manifest_path(path_dependency_manifest())
            .exec()
            .unwrap();
        let analyzer =
            RepositoryAnalyzer::from_metadata(metadata, Box::new(OfflineProvider::new()));
        let deps = analyzer.list_dependencies(true, true).unwrap();
        let helper = deps.iter().find(|d| d.name == "helper").unwrap();
        assert!(helper.transitive_deps.iter().any(|d| d == "nested_helper"));
    }
}
