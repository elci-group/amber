// Copyright (c) 2024 SVCH <svch@seriousaboutsolutions.co.uk>
// SPDX-License-Identifier: MIT
//! Recursive, provenance-aware software transformation planning.

pub mod contracts;
pub mod engine;
pub mod policy;
pub mod portfolio;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const TRANSFORMATION_PLAN_SCHEMA_VERSION: &str = "amber.transformation-plan/v1";
pub const DECISION_RECORD_SCHEMA_VERSION: &str = "amber.decision-record/v1";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TransformationStrategy {
    Preserve,
    Vendor,
    Mirror,
    Fork,
    SelectiveFork,
    Adapt,
    Extract,
    Redesign,
    Reimplement,
    Remove,
    Replace,
}

impl TransformationStrategy {
    #[must_use]
    pub const fn magnitude(self) -> f64 {
        match self {
            Self::Preserve => 0.0,
            Self::Vendor => 0.12,
            Self::Mirror => 0.18,
            Self::Fork => 0.35,
            Self::Adapt => 0.45,
            Self::SelectiveFork => 0.5,
            Self::Extract => 0.6,
            Self::Redesign => 0.8,
            Self::Reimplement => 0.9,
            Self::Remove | Self::Replace => 1.0,
        }
    }

    pub const ALL: [Self; 11] = [
        Self::Preserve,
        Self::Vendor,
        Self::Mirror,
        Self::Fork,
        Self::SelectiveFork,
        Self::Adapt,
        Self::Extract,
        Self::Redesign,
        Self::Reimplement,
        Self::Remove,
        Self::Replace,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DecisionStatus {
    Recommended,
    HumanReviewRequired,
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReplacementClassification {
    NotApplicable,
    BehaviouralReimplementation,
    CodeDerivedModification,
    ProvenanceUncertainReplacement,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    pub technical_value: f64,
    pub strategic_control: f64,
    pub reversibility: f64,
    pub licensing_risk: f64,
    pub provenance_risk: f64,
    pub drift_risk: f64,
    pub transformation_cost: f64,
    pub maintenance_cost: f64,
    pub integration_risk: f64,
    pub total: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateStrategy {
    pub strategy: TransformationStrategy,
    pub permitted: bool,
    pub constraint_reasons: Vec<String>,
    pub score: Option<ScoreBreakdown>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HumanReviewRequirement {
    pub target: String,
    pub code: String,
    pub reason: String,
    pub release_blocking: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReplacementLicensing {
    pub target: String,
    pub classification: ReplacementClassification,
    pub proposed_license: Option<String>,
    pub inherited_requirements: Vec<String>,
    pub human_review_required: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionRecord {
    pub schema_version: String,
    pub target: String,
    pub observations: Vec<String>,
    pub ontological_context: Vec<String>,
    pub licensing_evidence_ids: Vec<String>,
    pub provenance_evidence_ids: Vec<String>,
    pub technical_evidence: BTreeMap<String, f64>,
    pub drift_analysis: Vec<String>,
    pub candidate_strategies: Vec<CandidateStrategy>,
    pub constraints: Vec<String>,
    pub selected_strategy: Option<TransformationStrategy>,
    pub rationale: String,
    pub confidence: f64,
    pub status: DecisionStatus,
    pub child_decisions: Vec<String>,
    pub replacement_licensing: ReplacementLicensing,
    pub required_human_review: Vec<HumanReviewRequirement>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineageEdge {
    pub source: String,
    pub result: String,
    pub relationship: String,
    pub decision_target: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceContractVersions {
    pub lwoodz_evidence: String,
    pub lwoodz_licensing: String,
    pub lwoodz_provenance: String,
    pub padagonia_ontology: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransformationPlan {
    pub schema_version: String,
    pub root_target: String,
    pub policy_version: String,
    pub source_contract_versions: SourceContractVersions,
    pub decisions: Vec<DecisionRecord>,
    pub lineage: Vec<LineageEdge>,
    pub licensing_requirements: Vec<ReplacementLicensing>,
    pub provenance_requirements: Vec<String>,
    pub human_review_requirements: Vec<HumanReviewRequirement>,
}

impl TransformationPlan {
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

pub use engine::{
    DecisionFactors, EngineConfig, ScoreWeights, TransformationEngine, TransformationRequest,
};
pub use policy::{HardConstraint, LicensePolicy, LicensePolicyRule, PolicyDisposition};
