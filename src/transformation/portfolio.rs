// Copyright (c) 2024 SVCH <svch@seriousaboutsolutions.co.uk>
// SPDX-License-Identifier: MIT
//! Deterministic cross-dependency portfolio pass.

use super::TransformationStrategy;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PORTFOLIO_PLAN_SCHEMA_VERSION: &str = "amber.portfolio-plan/v1";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortfolioDependency {
    pub target: String,
    pub supplier: String,
    #[serde(default)]
    pub capabilities: BTreeSet<String>,
    pub selected_strategy: Option<TransformationStrategy>,
    pub annual_maintenance_cost: f64,
    pub licensing_risk: f64,
    pub supply_chain_risk: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortfolioConfig {
    pub supplier_concentration_threshold: f64,
    pub aggregate_licensing_risk_threshold: f64,
    pub duplicate_capability_threshold: usize,
}

impl Default for PortfolioConfig {
    fn default() -> Self {
        Self {
            supplier_concentration_threshold: 0.5,
            aggregate_licensing_risk_threshold: 0.6,
            duplicate_capability_threshold: 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortfolioFinding {
    pub code: String,
    pub targets: Vec<String>,
    pub rationale: String,
    pub requires_reconsideration: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortfolioPlan {
    pub schema_version: String,
    pub total_annual_maintenance_cost: f64,
    pub aggregate_licensing_risk: f64,
    pub aggregate_supply_chain_risk: f64,
    pub supplier_concentration: BTreeMap<String, f64>,
    pub strategy_distribution: BTreeMap<TransformationStrategy, usize>,
    pub findings: Vec<PortfolioFinding>,
}

#[derive(Clone, Debug)]
pub struct PortfolioOptimizer {
    config: PortfolioConfig,
}

impl PortfolioOptimizer {
    #[must_use]
    pub const fn new(config: PortfolioConfig) -> Self {
        Self { config }
    }

    #[must_use]
    pub fn analyse(&self, dependencies: &[PortfolioDependency]) -> PortfolioPlan {
        let count = dependencies.len().max(1) as f64;
        let total_annual_maintenance_cost = dependencies
            .iter()
            .map(|item| item.annual_maintenance_cost.max(0.0))
            .sum();
        let aggregate_licensing_risk = dependencies
            .iter()
            .map(|item| clamp(item.licensing_risk))
            .sum::<f64>()
            / count;
        let aggregate_supply_chain_risk = dependencies
            .iter()
            .map(|item| clamp(item.supply_chain_risk))
            .sum::<f64>()
            / count;
        let mut supplier_counts = BTreeMap::<String, usize>::new();
        let mut strategy_distribution = BTreeMap::new();
        let mut capabilities = BTreeMap::<String, Vec<String>>::new();
        for dependency in dependencies {
            *supplier_counts
                .entry(dependency.supplier.clone())
                .or_default() += 1;
            if let Some(strategy) = dependency.selected_strategy {
                *strategy_distribution.entry(strategy).or_default() += 1;
            }
            for capability in &dependency.capabilities {
                capabilities
                    .entry(capability.clone())
                    .or_default()
                    .push(dependency.target.clone());
            }
        }
        let supplier_concentration: BTreeMap<_, _> = supplier_counts
            .into_iter()
            .map(|(supplier, supplier_count)| (supplier, supplier_count as f64 / count))
            .collect();
        let mut findings = Vec::new();
        for (supplier, concentration) in &supplier_concentration {
            if *concentration > self.config.supplier_concentration_threshold {
                findings.push(PortfolioFinding {
                    code: "DEPENDENCY_CONCENTRATION".to_string(),
                    targets: dependencies
                        .iter()
                        .filter(|item| item.supplier == *supplier)
                        .map(|item| item.target.clone())
                        .collect(),
                    rationale: format!(
                        "supplier '{supplier}' represents {:.1}% of the dependency portfolio",
                        concentration * 100.0
                    ),
                    requires_reconsideration: true,
                });
            }
        }
        for (capability, mut targets) in capabilities {
            if targets.len() >= self.config.duplicate_capability_threshold {
                targets.sort();
                findings.push(PortfolioFinding {
                    code: "DUPLICATED_CAPABILITY".to_string(),
                    targets,
                    rationale: format!(
                        "capability '{capability}' is supplied by multiple dependencies"
                    ),
                    requires_reconsideration: true,
                });
            }
        }
        if aggregate_licensing_risk > self.config.aggregate_licensing_risk_threshold {
            findings.push(PortfolioFinding {
                code: "AGGREGATE_LICENSING_RISK".to_string(),
                targets: dependencies.iter().map(|item| item.target.clone()).collect(),
                rationale: format!(
                    "aggregate licensing risk {aggregate_licensing_risk:.3} exceeds the configured threshold"
                ),
                requires_reconsideration: true,
            });
        }
        findings.sort_by(|left, right| {
            left.code
                .cmp(&right.code)
                .then_with(|| left.targets.cmp(&right.targets))
        });
        PortfolioPlan {
            schema_version: PORTFOLIO_PLAN_SCHEMA_VERSION.to_string(),
            total_annual_maintenance_cost,
            aggregate_licensing_risk,
            aggregate_supply_chain_risk,
            supplier_concentration,
            strategy_distribution,
            findings,
        }
    }
}

fn clamp(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_collectively_undesirable_dependency_choices() {
        let dependencies = ["a", "b", "c"].map(|target| PortfolioDependency {
            target: target.to_string(),
            supplier: "one-supplier".to_string(),
            capabilities: ["parsing".to_string()].into_iter().collect(),
            selected_strategy: Some(TransformationStrategy::Preserve),
            annual_maintenance_cost: 10.0,
            licensing_risk: 0.7,
            supply_chain_risk: 0.5,
        });
        let plan = PortfolioOptimizer::new(PortfolioConfig::default()).analyse(&dependencies);
        let codes: BTreeSet<_> = plan
            .findings
            .iter()
            .map(|finding| finding.code.as_str())
            .collect();
        assert!(codes.contains("DEPENDENCY_CONCENTRATION"));
        assert!(codes.contains("DUPLICATED_CAPABILITY"));
        assert!(codes.contains("AGGREGATE_LICENSING_RISK"));
    }
}
