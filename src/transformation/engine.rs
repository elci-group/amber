// Copyright (c) 2024 SVCH <svch@seriousaboutsolutions.co.uk>
// SPDX-License-Identifier: MIT
//! Deterministic multi-objective strategy scoring and recursive composition.

use super::contracts::{
    LicenceFamily, LwoodzEvidenceSet, PadagoniaOntology, ProvenanceRelationship,
    LWOODZ_EVIDENCE_SCHEMA, LWOODZ_LICENSE_SCHEMA, LWOODZ_PROVENANCE_SCHEMA,
    PADAGONIA_ONTOLOGY_SCHEMA,
};
use super::policy::{LicensePolicy, PolicyDisposition};
use super::{
    CandidateStrategy, DecisionRecord, DecisionStatus, HumanReviewRequirement, LineageEdge,
    ReplacementClassification, ReplacementLicensing, ScoreBreakdown, SourceContractVersions,
    TransformationPlan, TransformationStrategy, DECISION_RECORD_SCHEMA_VERSION,
    TRANSFORMATION_PLAN_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionFactors {
    pub functional_suitability: f64,
    pub performance: f64,
    pub reliability: f64,
    pub security: f64,
    pub api_compatibility: f64,
    pub architectural_fit: f64,
    pub upstream_health: f64,
    pub maintainer_responsiveness: f64,
    pub security_responsiveness: f64,
    pub local_patch_burden: f64,
    pub drift_risk: f64,
    pub transformation_complexity: f64,
    pub test_burden: f64,
    pub migration_complexity: f64,
    pub ongoing_maintenance_cost: f64,
    pub strategic_control_need: f64,
    pub dependency_concentration: f64,
    pub reversibility: f64,
    pub coupling: f64,
}

impl Default for DecisionFactors {
    fn default() -> Self {
        Self {
            functional_suitability: 0.5,
            performance: 0.5,
            reliability: 0.5,
            security: 0.5,
            api_compatibility: 0.5,
            architectural_fit: 0.5,
            upstream_health: 0.5,
            maintainer_responsiveness: 0.5,
            security_responsiveness: 0.5,
            local_patch_burden: 0.5,
            drift_risk: 0.5,
            transformation_complexity: 0.5,
            test_burden: 0.5,
            migration_complexity: 0.5,
            ongoing_maintenance_cost: 0.5,
            strategic_control_need: 0.5,
            dependency_concentration: 0.5,
            reversibility: 0.5,
            coupling: 0.5,
        }
    }
}

impl DecisionFactors {
    fn technical_value(&self) -> f64 {
        mean(&[
            self.functional_suitability,
            self.performance,
            self.reliability,
            self.security,
            self.api_compatibility,
            self.architectural_fit,
        ])
    }

    fn transformation_cost(&self) -> f64 {
        mean(&[
            self.transformation_complexity,
            self.test_burden,
            self.migration_complexity,
        ])
    }

    fn maintenance_pressure(&self) -> f64 {
        mean(&[
            1.0 - clamp(self.upstream_health),
            1.0 - clamp(self.maintainer_responsiveness),
            1.0 - clamp(self.security_responsiveness),
            self.local_patch_burden,
            self.ongoing_maintenance_cost,
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScoreWeights {
    pub technical_value: f64,
    pub strategic_control: f64,
    pub reversibility: f64,
    pub licensing_risk: f64,
    pub provenance_risk: f64,
    pub drift_risk: f64,
    pub transformation_cost: f64,
    pub maintenance_cost: f64,
    pub integration_risk: f64,
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            technical_value: 1.0,
            strategic_control: 1.0,
            reversibility: 0.7,
            licensing_risk: 1.2,
            provenance_risk: 1.2,
            drift_risk: 0.8,
            transformation_cost: 1.0,
            maintenance_cost: 0.8,
            integration_risk: 0.9,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineConfig {
    pub minimum_decomposition_confidence: f64,
    pub minimum_granularity: u32,
    pub recurse_below_score: f64,
    pub high_impact_magnitude: f64,
    pub weights: ScoreWeights,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            minimum_decomposition_confidence: 0.7,
            minimum_granularity: 0,
            recurse_below_score: 0.25,
            high_impact_magnitude: 0.8,
            weights: ScoreWeights::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransformationRequest {
    pub evidence: LwoodzEvidenceSet,
    pub ontology: PadagoniaOntology,
    pub policy: LicensePolicy,
    #[serde(default)]
    pub factors: BTreeMap<String, DecisionFactors>,
    #[serde(default)]
    pub replacement_classifications: BTreeMap<String, ReplacementClassification>,
}

#[derive(Clone, Debug)]
pub struct TransformationEngine {
    config: EngineConfig,
}

impl TransformationEngine {
    #[must_use]
    pub const fn new(config: EngineConfig) -> Self {
        Self { config }
    }

    pub fn plan(&self, request: &TransformationRequest) -> Result<TransformationPlan, String> {
        request.evidence.validate()?;
        request.ontology.validate()?;
        request.policy.validate()?;
        if request.ontology.graph.root != request.evidence.license_assessment.target {
            return Err("lwoodz target and Padagonia root differ".to_string());
        }

        let mut decisions = self.evaluate_component(&request.ontology.graph.root, request)?;
        decisions.sort_by(|left, right| {
            left.target
                .matches('/')
                .count()
                .cmp(&right.target.matches('/').count())
                .then_with(|| left.target.cmp(&right.target))
        });

        let lineage = decisions.iter().filter_map(build_lineage_edge).collect();
        let licensing_requirements = decisions
            .iter()
            .map(|decision| decision.replacement_licensing.clone())
            .collect();
        let mut human_review_requirements: Vec<_> = decisions
            .iter()
            .flat_map(|decision| decision.required_human_review.clone())
            .collect();
        human_review_requirements.sort_by(|left, right| {
            left.target
                .cmp(&right.target)
                .then_with(|| left.code.cmp(&right.code))
        });
        human_review_requirements
            .dedup_by(|left, right| left.target == right.target && left.code == right.code);
        let provenance_requirements = decisions
            .iter()
            .filter(|decision| {
                matches!(
                    decision.replacement_licensing.classification,
                    ReplacementClassification::CodeDerivedModification
                        | ReplacementClassification::ProvenanceUncertainReplacement
                )
            })
            .map(|decision| {
                format!(
                    "retain evidence and lineage for {} until provenance review is complete",
                    decision.target
                )
            })
            .collect();

        Ok(TransformationPlan {
            schema_version: TRANSFORMATION_PLAN_SCHEMA_VERSION.to_string(),
            root_target: request.ontology.graph.root.clone(),
            policy_version: request.policy.version.clone(),
            source_contract_versions: SourceContractVersions {
                lwoodz_evidence: LWOODZ_EVIDENCE_SCHEMA.to_string(),
                lwoodz_licensing: LWOODZ_LICENSE_SCHEMA.to_string(),
                lwoodz_provenance: LWOODZ_PROVENANCE_SCHEMA.to_string(),
                padagonia_ontology: PADAGONIA_ONTOLOGY_SCHEMA.to_string(),
            },
            decisions,
            lineage,
            licensing_requirements,
            provenance_requirements,
            human_review_requirements,
        })
    }

    fn evaluate_component(
        &self,
        target: &str,
        request: &TransformationRequest,
    ) -> Result<Vec<DecisionRecord>, String> {
        let component = request
            .ontology
            .graph
            .component(target)
            .ok_or_else(|| format!("component '{target}' is missing"))?;
        let factors = request.factors.get(target).cloned().unwrap_or_default();
        let policy = request.policy.evaluate(target, &request.evidence);
        let licensing_risk = licensing_risk(target, &request.evidence);
        let provenance_risk = provenance_risk(target, &request.evidence);
        let mut candidates = TransformationStrategy::ALL
            .into_iter()
            .map(|strategy| {
                let permitted = policy.permitted.contains(&strategy)
                    && policy.disposition != PolicyDisposition::Reject;
                CandidateStrategy {
                    strategy,
                    permitted,
                    constraint_reasons: if permitted {
                        Vec::new()
                    } else {
                        policy.reasons.clone()
                    },
                    score: permitted
                        .then(|| self.score(strategy, &factors, licensing_risk, provenance_risk)),
                }
            })
            .collect::<Vec<_>>();
        candidates.sort_by(compare_candidates);

        let mut selected = candidates
            .iter()
            .find(|candidate| candidate.permitted)
            .map(|candidate| candidate.strategy);
        if policy.disposition == PolicyDisposition::Reject {
            selected = None;
        }
        let mut status = match policy.disposition {
            PolicyDisposition::Allow => DecisionStatus::Recommended,
            PolicyDisposition::HumanReview => DecisionStatus::HumanReviewRequired,
            PolicyDisposition::Reject => DecisionStatus::Rejected,
        };
        let mut reviews = Vec::new();
        if policy.disposition == PolicyDisposition::HumanReview {
            reviews.push(HumanReviewRequirement {
                target: target.to_string(),
                code: "LICENSING_OR_PROVENANCE_REVIEW".to_string(),
                reason: policy.reasons.join("; "),
                release_blocking: true,
            });
        }

        let children = request.ontology.graph.child_ids(target);
        let top_score = candidates
            .iter()
            .find_map(|candidate| candidate.score.as_ref().map(|score| score.total))
            .unwrap_or(f64::NEG_INFINITY);
        let can_descend = component.boundary.boundary_established
            && component.boundary.independently_addressable
            && component.boundary.granularity > self.config.minimum_granularity
            && request.ontology.graph.boundary_confidence(target)
                >= self.config.minimum_decomposition_confidence
            && !children.is_empty();
        let should_descend = can_descend
            && (request.evidence.has_issue_below(target)
                || children
                    .iter()
                    .any(|child| request.factors.contains_key(child))
                || top_score < self.config.recurse_below_score
                || selected.is_some_and(|strategy| strategy.magnitude() >= 0.45));

        let mut records = Vec::new();
        let mut child_strategies = Vec::new();
        if should_descend {
            for child in &children {
                let mut child_records = self.evaluate_component(child, request)?;
                if let Some(root) = child_records.iter().find(|record| record.target == *child) {
                    if let Some(strategy) = root.selected_strategy {
                        child_strategies.push(strategy);
                    }
                    if root.status == DecisionStatus::HumanReviewRequired {
                        status = DecisionStatus::HumanReviewRequired;
                    }
                }
                records.append(&mut child_records);
            }
            if let Some(aggregate) = aggregate_strategies(&child_strategies) {
                selected = Some(aggregate);
            }
        }

        if selected
            .is_some_and(|strategy| strategy.magnitude() >= self.config.high_impact_magnitude)
        {
            status = DecisionStatus::HumanReviewRequired;
            reviews.push(HumanReviewRequirement {
                target: target.to_string(),
                code: "HIGH_IMPACT_TRANSFORMATION".to_string(),
                reason: "selected strategy exceeds the configured high-impact threshold"
                    .to_string(),
                release_blocking: false,
            });
        }

        let replacement_licensing = replacement_licensing(
            target,
            selected,
            request,
            &policy.obligations,
            &mut reviews,
            &mut status,
        );
        let confidence = mean(&[
            request.evidence.license_assessment.confidence,
            request.evidence.provenance_assessment.confidence,
            request.ontology.graph.boundary_confidence(target),
        ]);
        let licensing_evidence_ids = request
            .evidence
            .facts_for(target)
            .into_iter()
            .flat_map(|fact| fact.evidence_ids.clone())
            .chain(
                request
                    .evidence
                    .conflicts_for(target)
                    .into_iter()
                    .flat_map(|conflict| conflict.evidence_ids.clone()),
            )
            .collect();
        let provenance_evidence_ids = request
            .evidence
            .provenance_assessment
            .links
            .iter()
            .filter(|link| link.subject == target)
            .flat_map(|link| link.evidence_ids.clone())
            .collect();
        let rationale = rationale(selected, should_descend, &child_strategies, &candidates);
        let record = DecisionRecord {
            schema_version: DECISION_RECORD_SCHEMA_VERSION.to_string(),
            target: target.to_string(),
            observations: observations(target, request),
            ontological_context: component.boundary.reasons.clone(),
            licensing_evidence_ids,
            provenance_evidence_ids,
            technical_evidence: technical_evidence(&factors),
            drift_analysis: vec![format!(
                "drift risk {:.3}; local patch burden {:.3}; upstream health {:.3}",
                clamp(factors.drift_risk),
                clamp(factors.local_patch_burden),
                clamp(factors.upstream_health)
            )],
            candidate_strategies: candidates,
            constraints: policy.reasons,
            selected_strategy: selected,
            rationale,
            confidence,
            status,
            child_decisions: if should_descend { children } else { Vec::new() },
            replacement_licensing,
            required_human_review: reviews,
        };
        records.push(record);
        Ok(records)
    }

    fn score(
        &self,
        strategy: TransformationStrategy,
        factors: &DecisionFactors,
        licensing_risk: f64,
        provenance_risk: f64,
    ) -> ScoreBreakdown {
        let profile = StrategyProfile::for_strategy(strategy);
        let weights = &self.config.weights;
        let technical_value = clamp(factors.technical_value()) * profile.technical_retention;
        let strategic_control = clamp(factors.strategic_control_need) * profile.control;
        let reversibility = clamp(factors.reversibility) * profile.reversibility;
        let licensing_risk = licensing_risk * profile.retained_code;
        let provenance_risk = provenance_risk * profile.retained_code;
        let drift_risk = clamp(factors.drift_risk) * profile.upstream_reliance;
        let transformation_cost = factors.transformation_cost() * strategy.magnitude();
        let maintenance_cost =
            factors.maintenance_pressure() * (0.2 + 0.8 * profile.local_ownership);
        let integration_risk = clamp(factors.coupling)
            * strategy.magnitude()
            * (0.5 + 0.5 * (1.0 - profile.reversibility));
        let total = technical_value * weights.technical_value
            + strategic_control * weights.strategic_control
            + reversibility * weights.reversibility
            - licensing_risk * weights.licensing_risk
            - provenance_risk * weights.provenance_risk
            - drift_risk * weights.drift_risk
            - transformation_cost * weights.transformation_cost
            - maintenance_cost * weights.maintenance_cost
            - integration_risk * weights.integration_risk;
        ScoreBreakdown {
            technical_value,
            strategic_control,
            reversibility,
            licensing_risk,
            provenance_risk,
            drift_risk,
            transformation_cost,
            maintenance_cost,
            integration_risk,
            total,
        }
    }
}

#[derive(Clone, Copy)]
struct StrategyProfile {
    technical_retention: f64,
    control: f64,
    reversibility: f64,
    retained_code: f64,
    upstream_reliance: f64,
    local_ownership: f64,
}

impl StrategyProfile {
    const fn for_strategy(strategy: TransformationStrategy) -> Self {
        match strategy {
            TransformationStrategy::Preserve => Self::new(1.0, 0.0, 1.0, 1.0, 1.0, 0.0),
            TransformationStrategy::Vendor => Self::new(1.0, 0.2, 0.8, 1.0, 0.8, 0.3),
            TransformationStrategy::Mirror => Self::new(1.0, 0.25, 0.9, 1.0, 0.7, 0.2),
            TransformationStrategy::Fork => Self::new(1.0, 0.6, 0.55, 1.0, 0.45, 0.65),
            TransformationStrategy::SelectiveFork => Self::new(0.98, 0.7, 0.6, 0.65, 0.35, 0.7),
            TransformationStrategy::Adapt => Self::new(0.95, 0.55, 0.7, 0.8, 0.55, 0.55),
            TransformationStrategy::Extract => Self::new(0.9, 0.75, 0.5, 0.55, 0.25, 0.8),
            TransformationStrategy::Redesign => Self::new(0.85, 0.9, 0.35, 0.2, 0.1, 0.95),
            TransformationStrategy::Reimplement => Self::new(0.9, 1.0, 0.5, 0.0, 0.0, 1.0),
            TransformationStrategy::Remove => Self::new(0.0, 1.0, 0.7, 0.0, 0.0, 0.0),
            TransformationStrategy::Replace => Self::new(0.9, 0.6, 0.4, 0.0, 0.25, 0.25),
        }
    }

    const fn new(
        technical_retention: f64,
        control: f64,
        reversibility: f64,
        retained_code: f64,
        upstream_reliance: f64,
        local_ownership: f64,
    ) -> Self {
        Self {
            technical_retention,
            control,
            reversibility,
            retained_code,
            upstream_reliance,
            local_ownership,
        }
    }
}

fn compare_candidates(left: &CandidateStrategy, right: &CandidateStrategy) -> Ordering {
    let left_score = left
        .score
        .as_ref()
        .map_or(f64::NEG_INFINITY, |score| score.total);
    let right_score = right
        .score
        .as_ref()
        .map_or(f64::NEG_INFINITY, |score| score.total);
    right_score
        .partial_cmp(&left_score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| left.strategy.cmp(&right.strategy))
}

fn licensing_risk(target: &str, evidence: &LwoodzEvidenceSet) -> f64 {
    if !evidence.conflicts_for(target).is_empty() {
        return 1.0;
    }
    let facts = evidence.facts_for(target);
    if facts.is_empty() {
        return 0.85;
    }
    facts
        .into_iter()
        .map(|fact| match fact.family {
            LicenceFamily::Permissive => 0.1,
            LicenceFamily::WeakCopyleft => 0.4,
            LicenceFamily::StrongCopyleft => 0.75,
            LicenceFamily::NetworkCopyleft => 0.9,
            LicenceFamily::Proprietary => 1.0,
            LicenceFamily::Unknown | LicenceFamily::Unverified => 0.85,
            LicenceFamily::Conflicting => 1.0,
        })
        .fold(0.0, f64::max)
}

fn provenance_risk(target: &str, evidence: &LwoodzEvidenceSet) -> f64 {
    if evidence
        .provenance_assessment
        .uncertain_subjects
        .iter()
        .any(|subject| subject == target)
    {
        return 1.0;
    }
    evidence
        .provenance_assessment
        .links
        .iter()
        .filter(|link| link.subject == target)
        .map(|link| {
            let multiplier = match link.relationship {
                ProvenanceRelationship::CopiedFrom | ProvenanceRelationship::DerivesFrom => 1.0,
                ProvenanceRelationship::VendoredFrom | ProvenanceRelationship::Duplicates => 0.9,
                ProvenanceRelationship::GeneratedFrom | ProvenanceRelationship::Wraps => 0.65,
                ProvenanceRelationship::SimilarTo | ProvenanceRelationship::Unknown => 0.75,
            };
            link.confidence * multiplier
        })
        .fold(0.2, f64::max)
}

fn aggregate_strategies(strategies: &[TransformationStrategy]) -> Option<TransformationStrategy> {
    let first = *strategies.first()?;
    if strategies.iter().all(|strategy| *strategy == first) {
        return Some(first);
    }
    let retains_some = strategies.iter().any(|strategy| {
        matches!(
            strategy,
            TransformationStrategy::Preserve
                | TransformationStrategy::Vendor
                | TransformationStrategy::Mirror
                | TransformationStrategy::Fork
                | TransformationStrategy::Adapt
        )
    });
    let transforms_some = strategies.iter().any(|strategy| {
        matches!(
            strategy,
            TransformationStrategy::Extract
                | TransformationStrategy::Redesign
                | TransformationStrategy::Reimplement
                | TransformationStrategy::Remove
                | TransformationStrategy::Replace
        )
    });
    if retains_some && transforms_some
        || strategies.iter().any(|strategy| {
            matches!(
                strategy,
                TransformationStrategy::Fork | TransformationStrategy::SelectiveFork
            )
        })
    {
        Some(TransformationStrategy::SelectiveFork)
    } else {
        Some(TransformationStrategy::Adapt)
    }
}

fn replacement_licensing(
    target: &str,
    selected: Option<TransformationStrategy>,
    request: &TransformationRequest,
    obligations: &[String],
    reviews: &mut Vec<HumanReviewRequirement>,
    status: &mut DecisionStatus,
) -> ReplacementLicensing {
    let classification = request
        .replacement_classifications
        .get(target)
        .copied()
        .unwrap_or_else(|| match selected {
            Some(TransformationStrategy::Fork)
            | Some(TransformationStrategy::SelectiveFork)
            | Some(TransformationStrategy::Adapt)
            | Some(TransformationStrategy::Extract) => {
                ReplacementClassification::CodeDerivedModification
            }
            Some(TransformationStrategy::Redesign)
            | Some(TransformationStrategy::Reimplement)
            | Some(TransformationStrategy::Replace) => {
                ReplacementClassification::ProvenanceUncertainReplacement
            }
            _ => ReplacementClassification::NotApplicable,
        });
    let proposed_license = (classification
        == ReplacementClassification::BehaviouralReimplementation)
        .then(|| request.policy.default_replacement_license.clone());
    let human_review_required = matches!(
        classification,
        ReplacementClassification::CodeDerivedModification
            | ReplacementClassification::ProvenanceUncertainReplacement
    );
    if human_review_required {
        *status = DecisionStatus::HumanReviewRequired;
        reviews.push(HumanReviewRequirement {
            target: target.to_string(),
            code: "REPLACEMENT_PROVENANCE_REVIEW".to_string(),
            reason: "replacement provenance does not establish an independently implemented work"
                .to_string(),
            release_blocking: true,
        });
    }
    ReplacementLicensing {
        target: target.to_string(),
        classification,
        proposed_license,
        inherited_requirements: if classification
            == ReplacementClassification::CodeDerivedModification
        {
            obligations.to_vec()
        } else {
            Vec::new()
        },
        human_review_required,
    }
}

fn observations(target: &str, request: &TransformationRequest) -> Vec<String> {
    let declared = request
        .evidence
        .facts_for(target)
        .into_iter()
        .filter(|fact| fact.origin == super::contracts::EvidenceOrigin::Declared)
        .map(|fact| fact.expression.clone())
        .collect::<Vec<_>>();
    let observed = request
        .evidence
        .facts_for(target)
        .into_iter()
        .filter(|fact| fact.origin == super::contracts::EvidenceOrigin::Observed)
        .map(|fact| fact.expression.clone())
        .collect::<Vec<_>>();
    vec![
        format!("declared licence expressions: {}", declared.join(", ")),
        format!("observed licence expressions: {}", observed.join(", ")),
        format!(
            "licensing conflicts: {}",
            request.evidence.conflicts_for(target).len()
        ),
    ]
}

fn technical_evidence(factors: &DecisionFactors) -> BTreeMap<String, f64> {
    [
        ("technical_value", factors.technical_value()),
        ("transformation_cost", factors.transformation_cost()),
        ("maintenance_pressure", factors.maintenance_pressure()),
        ("coupling", clamp(factors.coupling)),
        ("reversibility", clamp(factors.reversibility)),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value))
    .collect()
}

fn rationale(
    selected: Option<TransformationStrategy>,
    decomposed: bool,
    child_strategies: &[TransformationStrategy],
    candidates: &[CandidateStrategy],
) -> String {
    let Some(strategy) = selected else {
        return "deterministic policy constraints reject an automatic recommendation".to_string();
    };
    if decomposed {
        return format!(
            "{strategy:?} is the compositional result of {} independently evaluated child boundaries",
            child_strategies.len()
        );
    }
    let score = candidates
        .iter()
        .find(|candidate| candidate.strategy == strategy)
        .and_then(|candidate| candidate.score.as_ref())
        .map_or(0.0, |item| item.total);
    format!(
        "{strategy:?} has the highest permitted deterministic multi-objective score ({score:.3})"
    )
}

fn build_lineage_edge(decision: &DecisionRecord) -> Option<LineageEdge> {
    let strategy = decision.selected_strategy?;
    let (relationship, suffix) = match strategy {
        TransformationStrategy::Preserve => ("preserved_as", "upstream"),
        TransformationStrategy::Vendor | TransformationStrategy::Mirror => {
            ("mirrored_as", "mirror")
        }
        TransformationStrategy::Fork | TransformationStrategy::SelectiveFork => {
            ("forked_as", "local-fork")
        }
        TransformationStrategy::Adapt | TransformationStrategy::Extract => {
            ("derived_as", "local-adaptation")
        }
        TransformationStrategy::Redesign => ("redesigned_as", "redesign"),
        TransformationStrategy::Reimplement => {
            ("implemented_independently_as", "independent-implementation")
        }
        TransformationStrategy::Remove => ("removed_by", "removal"),
        TransformationStrategy::Replace => ("replaced_by", "replacement"),
    };
    Some(LineageEdge {
        source: decision.target.clone(),
        result: format!("{}#{suffix}", decision.target),
        relationship: relationship.to_string(),
        decision_target: decision.target.clone(),
        evidence_ids: decision
            .licensing_evidence_ids
            .iter()
            .chain(&decision.provenance_evidence_ids)
            .cloned()
            .collect(),
    })
}

fn mean(values: &[f64]) -> f64 {
    values.iter().map(|value| clamp(*value)).sum::<f64>() / values.len() as f64
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
    use crate::transformation::contracts::{
        ComponentGraph, DecompositionConfidence, EvidenceOrigin, LicenceFact, LicenseAssessment,
        LicensingConflict, ProvenanceAssessment, RelationshipSet, SoftwareComponent,
        SoftwareRelationship, SoftwareRelationshipKind, TransformationBoundary,
    };

    fn component(id: &str, granularity: u32) -> SoftwareComponent {
        SoftwareComponent {
            id: id.to_string(),
            name: id.to_string(),
            kind: serde_json::json!("component"),
            path: None,
            boundary: TransformationBoundary {
                independently_addressable: true,
                independently_transformable: true,
                boundary_established: true,
                granularity,
                reasons: vec!["stable module boundary".to_string()],
            },
            attributes: BTreeMap::new(),
            evidence_ids: Vec::new(),
        }
    }

    fn request() -> TransformationRequest {
        let root_fact = LicenceFact {
            subject: "dependency-x".to_string(),
            expression: "MIT".to_string(),
            spdx_identifiers: vec!["MIT".to_string()],
            family: LicenceFamily::Permissive,
            origin: EvidenceOrigin::Declared,
            evidence_ids: vec!["lic-root".to_string()],
            confidence: 1.0,
        };
        let child_fact = LicenceFact {
            subject: "dependency-x/component-f".to_string(),
            expression: "GPL-3.0-only".to_string(),
            spdx_identifiers: vec!["GPL-3.0-only".to_string()],
            family: LicenceFamily::StrongCopyleft,
            origin: EvidenceOrigin::Observed,
            evidence_ids: vec!["lic-f".to_string()],
            confidence: 0.98,
        };
        let child_a_fact = LicenceFact {
            subject: "dependency-x/component-a".to_string(),
            expression: "MIT".to_string(),
            spdx_identifiers: vec!["MIT".to_string()],
            family: LicenceFamily::Permissive,
            origin: EvidenceOrigin::Observed,
            evidence_ids: vec!["lic-a".to_string()],
            confidence: 0.99,
        };
        let evidence = LwoodzEvidenceSet {
            schema_version: LWOODZ_EVIDENCE_SCHEMA.to_string(),
            license_assessment: LicenseAssessment {
                schema_version: LWOODZ_LICENSE_SCHEMA.to_string(),
                target: "dependency-x".to_string(),
                licences: vec![root_fact, child_a_fact, child_fact],
                conflicts: vec![LicensingConflict {
                    code: "LICENSING_PROVENANCE_CONFLICT".to_string(),
                    subject: "dependency-x/component-f".to_string(),
                    declared_expressions: vec!["MIT".to_string()],
                    observed_expressions: vec!["GPL-3.0-only".to_string()],
                    evidence_ids: vec!["lic-f".to_string()],
                    confidence: 0.98,
                }],
                confidence: 0.98,
            },
            provenance_assessment: ProvenanceAssessment {
                schema_version: LWOODZ_PROVENANCE_SCHEMA.to_string(),
                target: "dependency-x".to_string(),
                links: Vec::new(),
                uncertain_subjects: Vec::new(),
                confidence: 0.95,
            },
            observations: Vec::new(),
        };
        let ontology = PadagoniaOntology {
            schema_version: PADAGONIA_ONTOLOGY_SCHEMA.to_string(),
            namespace: "test".to_string(),
            graph: ComponentGraph {
                root: "dependency-x".to_string(),
                components: vec![
                    component("dependency-x", 2),
                    component("dependency-x/component-a", 1),
                    component("dependency-x/component-f", 1),
                ],
                relationships: RelationshipSet {
                    relationships: vec![
                        SoftwareRelationship {
                            source: "dependency-x".to_string(),
                            target: "dependency-x/component-a".to_string(),
                            kind: SoftwareRelationshipKind::Contains,
                            confidence: 0.95,
                            evidence_ids: Vec::new(),
                        },
                        SoftwareRelationship {
                            source: "dependency-x".to_string(),
                            target: "dependency-x/component-f".to_string(),
                            kind: SoftwareRelationshipKind::Contains,
                            confidence: 0.95,
                            evidence_ids: Vec::new(),
                        },
                    ],
                },
                decomposition_confidence: DecompositionConfidence {
                    overall: 0.95,
                    component_boundaries: BTreeMap::new(),
                    unresolved_boundaries: Vec::new(),
                },
            },
            vocabulary: BTreeMap::new(),
        };
        let mut factors = BTreeMap::new();
        factors.insert(
            "dependency-x/component-f".to_string(),
            DecisionFactors {
                transformation_complexity: 0.05,
                test_burden: 0.1,
                migration_complexity: 0.05,
                coupling: 0.1,
                strategic_control_need: 0.9,
                ..DecisionFactors::default()
            },
        );
        let mut replacement_classifications = BTreeMap::new();
        replacement_classifications.insert(
            "dependency-x/component-f".to_string(),
            ReplacementClassification::BehaviouralReimplementation,
        );
        TransformationRequest {
            evidence,
            ontology,
            policy: LicensePolicy::baseline("MIT"),
            factors,
            replacement_classifications,
        }
    }

    #[test]
    fn produces_recursive_machine_readable_plan() {
        let plan = TransformationEngine::new(EngineConfig::default())
            .plan(&request())
            .expect("plan should be generated");
        let root = plan
            .decisions
            .iter()
            .find(|decision| decision.target == "dependency-x")
            .expect("root decision");
        assert_eq!(root.child_decisions.len(), 2);
        assert_eq!(
            root.selected_strategy,
            Some(TransformationStrategy::SelectiveFork)
        );
        let component_f = plan
            .decisions
            .iter()
            .find(|decision| decision.target.ends_with("component-f"))
            .expect("component decision");
        assert_eq!(
            component_f.replacement_licensing.classification,
            ReplacementClassification::BehaviouralReimplementation
        );
        assert_eq!(
            component_f
                .replacement_licensing
                .proposed_license
                .as_deref(),
            Some("MIT")
        );
        assert!(plan.to_json_pretty().is_ok());
    }

    #[test]
    fn hard_reject_precedes_scores() {
        let mut request = request();
        request
            .policy
            .hard_constraints
            .push(super::super::policy::HardConstraint::LicenceFamily {
                id: "reject-strong".to_string(),
                family: LicenceFamily::StrongCopyleft,
                disposition: PolicyDisposition::Reject,
                reason: "release policy prohibits this family".to_string(),
            });
        let plan = TransformationEngine::new(EngineConfig::default())
            .plan(&request)
            .expect("plan should be generated");
        let component_f = plan
            .decisions
            .iter()
            .find(|decision| decision.target.ends_with("component-f"))
            .expect("component decision");
        assert_eq!(component_f.status, DecisionStatus::Rejected);
        assert!(component_f.selected_strategy.is_none());
    }
}
