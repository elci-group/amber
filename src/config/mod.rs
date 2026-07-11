//! Configuration and policy enforcement.
//!
//! Amber can read a `.amber.toml` file from the target project directory to
//! customize scoring weights, thresholds, and policies.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Top-level configuration file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    /// Minimum score threshold for replacement proposals.
    pub threshold: Option<u8>,
    /// Per-dimension scoring weights.
    pub weights: Option<Weights>,
    /// Policy enforcement.
    pub policy: Option<Policy>,
    /// Padagonia-backed replacement module library.
    pub library: Option<LibraryConfig>,
}

/// Configuration for the replacement module library.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryConfig {
    /// Whether to use the library for replacement lookup and storage.
    #[serde(default)]
    pub enabled: bool,
    /// Path to the Padagonia library file.
    pub path: Option<PathBuf>,
}

impl LibraryConfig {
    /// Resolve the library path, defaulting to `~/.amber/library.pad`.
    #[must_use]
    pub fn resolved_path(&self) -> PathBuf {
        if let Some(path) = &self.path {
            return expand_home(path);
        }
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
        home.join(".amber").join("library.pad")
    }
}

fn expand_home(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Weights {
    pub usage_simplicity: f64,
    pub transitive_value: f64,
    pub security_safety: f64,
    pub maintenance_burden: f64,
    pub testability: f64,
    pub api_surface: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            usage_simplicity: 0.25,
            transitive_value: 0.20,
            security_safety: 0.25,
            maintenance_burden: 0.10,
            testability: 0.10,
            api_surface: 0.10,
        }
    }
}

impl Weights {
    /// Validate that all weights are non-negative and sum to approximately 1.0.
    ///
    /// # Errors
    ///
    /// Returns an error message if any weight is negative or the total deviates
    /// from `1.0` by more than `1e-6`.
    pub fn validate(&self) -> Result<(), String> {
        if !self.usage_simplicity.is_finite()
            || !self.transitive_value.is_finite()
            || !self.security_safety.is_finite()
            || !self.maintenance_burden.is_finite()
            || !self.testability.is_finite()
            || !self.api_surface.is_finite()
        {
            return Err("weights must be finite numbers".to_string());
        }

        if self.usage_simplicity < 0.0
            || self.transitive_value < 0.0
            || self.security_safety < 0.0
            || self.maintenance_burden < 0.0
            || self.testability < 0.0
            || self.api_surface < 0.0
        {
            return Err("weights must be non-negative".to_string());
        }

        let total = self.usage_simplicity
            + self.transitive_value
            + self.security_safety
            + self.maintenance_burden
            + self.testability
            + self.api_surface;

        if (total - 1.0).abs() > 1e-6 {
            return Err(format!("weights must sum to 1.0, got {total}"));
        }

        Ok(())
    }
}

impl Config {
    /// Validate the configuration, returning an error if any section is invalid.
    ///
    /// # Errors
    ///
    /// Returns a descriptive error if weights or thresholds are invalid.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(threshold) = self.threshold {
            if threshold > 100 {
                return Err(format!(
                    "threshold must be between 0 and 100, got {threshold}"
                ));
            }
        }

        if let Some(weights) = &self.weights {
            weights.validate()?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Policy {
    /// Crates that must remain in the project.
    pub required: Vec<String>,
    /// Crates that must never be introduced.
    pub forbidden: Vec<String>,
    /// Treat policy violations as errors.
    #[serde(default)]
    pub strict: bool,
}

impl Policy {
    /// Check if a crate is required.
    #[must_use]
    pub fn is_required(&self, name: &str) -> bool {
        self.required.iter().any(|r| r == name)
    }

    /// Check if a crate is forbidden.
    #[must_use]
    pub fn is_forbidden(&self, name: &str) -> bool {
        self.forbidden.iter().any(|f| f == name)
    }

    /// Validate a set of dependency names against the policy.
    #[must_use]
    pub fn validate(&self, deps: &HashSet<String>) -> Vec<PolicyViolation> {
        let mut violations = Vec::new();

        for required in &self.required {
            if !deps.contains(required) {
                violations.push(PolicyViolation::MissingRequired(required.clone()));
            }
        }

        for dep in deps {
            if self.is_forbidden(dep) {
                violations.push(PolicyViolation::Forbidden(dep.clone()));
            }
        }

        violations
    }
}

/// A policy violation detected during analysis.
#[derive(Debug, Clone)]
pub enum PolicyViolation {
    MissingRequired(String),
    Forbidden(String),
}

impl PolicyViolation {
    /// Human-readable description of the violation.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::MissingRequired(name) => {
                format!("Required dependency '{name}' is missing")
            }
            Self::Forbidden(name) => {
                format!("Forbidden dependency '{name}' is present")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn policy_detects_missing_required() {
        let policy = Policy {
            required: vec!["serde".to_string()],
            forbidden: Vec::new(),
            strict: false,
        };
        let deps: HashSet<String> = std::iter::once("anyhow".to_string()).collect();
        let violations = policy.validate(&deps);
        assert_eq!(violations.len(), 1);
        assert!(matches!(
            &violations[0],
            PolicyViolation::MissingRequired(name) if name == "serde"
        ));
    }

    #[test]
    fn policy_detects_forbidden() {
        let policy = Policy {
            required: Vec::new(),
            forbidden: vec!["deprecated_crate".to_string()],
            strict: false,
        };
        let deps: HashSet<String> = ["anyhow".to_string(), "deprecated_crate".to_string()]
            .into_iter()
            .collect();
        let violations = policy.validate(&deps);
        assert_eq!(violations.len(), 1);
        assert!(matches!(
            &violations[0],
            PolicyViolation::Forbidden(name) if name == "deprecated_crate"
        ));
    }

    #[test]
    fn policy_passes_when_clean() {
        let policy = Policy {
            required: vec!["serde".to_string()],
            forbidden: vec!["deprecated_crate".to_string()],
            strict: false,
        };
        let deps: HashSet<String> = ["serde".to_string(), "anyhow".to_string()]
            .into_iter()
            .collect();
        assert!(policy.validate(&deps).is_empty());
    }

    #[test]
    fn policy_violation_messages_are_descriptive() {
        let missing = PolicyViolation::MissingRequired("serde".to_string());
        assert!(missing.message().contains("Required dependency"));

        let forbidden = PolicyViolation::Forbidden("bad".to_string());
        assert!(forbidden.message().contains("Forbidden dependency"));
    }

    #[test]
    fn default_weights_sum_to_one() {
        let weights = Weights::default();
        let sum = weights.usage_simplicity
            + weights.transitive_value
            + weights.security_safety
            + weights.maintenance_burden
            + weights.testability
            + weights.api_surface;
        assert!((sum - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn policy_required_and_forbidden_checks_match() {
        let policy = Policy {
            required: vec!["serde".to_string()],
            forbidden: vec!["bad".to_string()],
            strict: false,
        };
        assert!(policy.is_required("serde"));
        assert!(!policy.is_required("other"));
        assert!(policy.is_forbidden("bad"));
        assert!(!policy.is_forbidden("other"));
    }

    #[test]
    fn weights_validation_accepts_default() {
        assert!(Weights::default().validate().is_ok());
    }

    #[test]
    fn weights_validation_rejects_negative() {
        let weights = Weights {
            usage_simplicity: -0.1,
            security_safety: 1.35, // keep sum at 1.0
            ..Weights::default()
        };
        assert!(weights.validate().is_err());
    }

    #[test]
    fn weights_validation_rejects_invalid_sum() {
        let weights = Weights {
            usage_simplicity: 0.30,
            ..Weights::default()
        };
        assert!(weights.validate().is_err());
    }

    #[test]
    fn weights_validation_rejects_nan() {
        let weights = Weights {
            usage_simplicity: f64::NAN,
            ..Weights::default()
        };
        assert!(weights.validate().is_err());
    }

    #[test]
    fn weights_validation_rejects_infinity() {
        let weights = Weights {
            usage_simplicity: f64::INFINITY,
            ..Weights::default()
        };
        assert!(weights.validate().is_err());

        let weights = Weights {
            usage_simplicity: f64::NEG_INFINITY,
            ..Weights::default()
        };
        assert!(weights.validate().is_err());
    }

    #[test]
    fn config_validation_rejects_high_threshold() {
        let config = Config {
            threshold: Some(101),
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn config_validation_accepts_defaults() {
        assert!(Config::default().validate().is_ok());
    }
}
