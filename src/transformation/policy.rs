// Copyright (c) 2024 SVCH <svch@seriousaboutsolutions.co.uk>
// SPDX-License-Identifier: MIT
//! Configurable deterministic licensing policy constraints.

use super::contracts::{LicenceFamily, LwoodzEvidenceSet};
use super::TransformationStrategy;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PolicyDisposition {
    Allow,
    HumanReview,
    Reject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LicensePolicyRule {
    pub id: String,
    #[serde(default)]
    pub spdx_identifiers: BTreeSet<String>,
    #[serde(default)]
    pub families: BTreeSet<LicenceFamily>,
    #[serde(default)]
    pub permitted_transformations: BTreeSet<TransformationStrategy>,
    #[serde(default)]
    pub prohibited_transformations: BTreeSet<TransformationStrategy>,
    #[serde(default)]
    pub obligations: Vec<String>,
    pub disposition: PolicyDisposition,
    pub preferred_replacement: Option<TransformationStrategy>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "when", rename_all = "snake_case")]
pub enum HardConstraint {
    LicensingConflict {
        id: String,
        disposition: PolicyDisposition,
        reason: String,
    },
    ProvenanceConfidenceBelow {
        id: String,
        threshold: f64,
        disposition: PolicyDisposition,
        reason: String,
    },
    LicenceFamily {
        id: String,
        family: LicenceFamily,
        disposition: PolicyDisposition,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LicensePolicy {
    pub version: String,
    pub default_replacement_license: String,
    pub rules: Vec<LicensePolicyRule>,
    pub hard_constraints: Vec<HardConstraint>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolicyEvaluation {
    pub disposition: PolicyDisposition,
    pub permitted: BTreeSet<TransformationStrategy>,
    pub prohibited: BTreeSet<TransformationStrategy>,
    pub obligations: Vec<String>,
    pub reasons: Vec<String>,
    pub preferred_replacement: Option<TransformationStrategy>,
}

impl LicensePolicy {
    /// A conservative, editable starting policy. Callers should persist and
    /// version their own policy rather than treating this as legal advice.
    #[must_use]
    pub fn baseline(default_replacement_license: impl Into<String>) -> Self {
        let all: BTreeSet<_> = TransformationStrategy::ALL.into_iter().collect();
        let permissive = LicensePolicyRule {
            id: "baseline-permissive".to_string(),
            spdx_identifiers: BTreeSet::new(),
            families: [LicenceFamily::Permissive].into_iter().collect(),
            permitted_transformations: all.clone(),
            prohibited_transformations: BTreeSet::new(),
            obligations: vec!["preserve applicable attribution and notices".to_string()],
            disposition: PolicyDisposition::Allow,
            preferred_replacement: None,
        };
        let copyleft = LicensePolicyRule {
            id: "baseline-copyleft".to_string(),
            spdx_identifiers: BTreeSet::new(),
            families: [
                LicenceFamily::WeakCopyleft,
                LicenceFamily::StrongCopyleft,
                LicenceFamily::NetworkCopyleft,
            ]
            .into_iter()
            .collect(),
            permitted_transformations: all,
            prohibited_transformations: BTreeSet::new(),
            obligations: vec![
                "carry forward applicable source, notice, and modification obligations".to_string(),
            ],
            disposition: PolicyDisposition::HumanReview,
            preferred_replacement: None,
        };
        Self {
            version: "amber.license-policy/baseline-v1".to_string(),
            default_replacement_license: default_replacement_license.into(),
            rules: vec![permissive, copyleft],
            hard_constraints: vec![
                HardConstraint::LicensingConflict {
                    id: "conflict-review".to_string(),
                    disposition: PolicyDisposition::HumanReview,
                    reason: "declared licensing conflicts with component evidence".to_string(),
                },
                HardConstraint::ProvenanceConfidenceBelow {
                    id: "low-provenance-review".to_string(),
                    threshold: 0.75,
                    disposition: PolicyDisposition::HumanReview,
                    reason: "provenance confidence is below policy threshold".to_string(),
                },
                HardConstraint::LicenceFamily {
                    id: "unknown-license-review".to_string(),
                    family: LicenceFamily::Unknown,
                    disposition: PolicyDisposition::HumanReview,
                    reason: "licence family is unknown".to_string(),
                },
                HardConstraint::LicenceFamily {
                    id: "unverified-license-review".to_string(),
                    family: LicenceFamily::Unverified,
                    disposition: PolicyDisposition::HumanReview,
                    reason: "licensing evidence is unverified".to_string(),
                },
            ],
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version.trim().is_empty() {
            return Err("policy version is required".to_string());
        }
        if self.default_replacement_license.trim().is_empty() {
            return Err("default replacement licence is required".to_string());
        }
        for constraint in &self.hard_constraints {
            if let HardConstraint::ProvenanceConfidenceBelow { threshold, .. } = constraint {
                if !threshold.is_finite() || !(0.0..=1.0).contains(threshold) {
                    return Err("provenance threshold must be between zero and one".to_string());
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn evaluate(&self, target: &str, evidence: &LwoodzEvidenceSet) -> PolicyEvaluation {
        let facts = evidence.facts_for(target);
        let mut evaluation = PolicyEvaluation {
            disposition: PolicyDisposition::Allow,
            permitted: TransformationStrategy::ALL.into_iter().collect(),
            prohibited: BTreeSet::new(),
            obligations: Vec::new(),
            reasons: Vec::new(),
            preferred_replacement: None,
        };

        let mut matched = false;
        for rule in &self.rules {
            let rule_matches = facts.iter().any(|fact| {
                rule.families.contains(&fact.family)
                    || fact
                        .spdx_identifiers
                        .iter()
                        .any(|identifier| rule.spdx_identifiers.contains(identifier))
            });
            if !rule_matches {
                continue;
            }
            matched = true;
            if !rule.permitted_transformations.is_empty() {
                evaluation.permitted = evaluation
                    .permitted
                    .intersection(&rule.permitted_transformations)
                    .copied()
                    .collect();
            }
            evaluation
                .prohibited
                .extend(&rule.prohibited_transformations);
            evaluation
                .obligations
                .extend(rule.obligations.iter().cloned());
            evaluation.disposition = evaluation.disposition.max(rule.disposition);
            evaluation.preferred_replacement = rule
                .preferred_replacement
                .or(evaluation.preferred_replacement);
            evaluation
                .reasons
                .push(format!("matched licensing policy rule '{}'", rule.id));
        }

        if !matched {
            evaluation.disposition = PolicyDisposition::HumanReview;
            evaluation
                .reasons
                .push("no licensing policy rule matched the observed facts".to_string());
        }

        for constraint in &self.hard_constraints {
            let (applies, disposition, id, reason) = match constraint {
                HardConstraint::LicensingConflict {
                    id,
                    disposition,
                    reason,
                } => (
                    !evidence.conflicts_for(target).is_empty(),
                    *disposition,
                    id,
                    reason,
                ),
                HardConstraint::ProvenanceConfidenceBelow {
                    id,
                    threshold,
                    disposition,
                    reason,
                } => (
                    evidence.provenance_assessment.confidence < *threshold
                        || evidence
                            .provenance_assessment
                            .uncertain_subjects
                            .iter()
                            .any(|subject| subject == target),
                    *disposition,
                    id,
                    reason,
                ),
                HardConstraint::LicenceFamily {
                    id,
                    family,
                    disposition,
                    reason,
                } => (
                    facts.iter().any(|fact| fact.family == *family),
                    *disposition,
                    id,
                    reason,
                ),
            };
            if applies {
                evaluation.disposition = evaluation.disposition.max(disposition);
                evaluation
                    .reasons
                    .push(format!("hard constraint '{id}': {reason}"));
            }
        }

        evaluation.permitted = evaluation
            .permitted
            .difference(&evaluation.prohibited)
            .copied()
            .collect();
        evaluation.obligations.sort();
        evaluation.obligations.dedup();
        evaluation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transformation::contracts::{
        LicenseAssessment, ProvenanceAssessment, LWOODZ_EVIDENCE_SCHEMA, LWOODZ_LICENSE_SCHEMA,
        LWOODZ_PROVENANCE_SCHEMA,
    };

    #[test]
    fn unmatched_licence_requires_review() {
        let evidence = LwoodzEvidenceSet {
            schema_version: LWOODZ_EVIDENCE_SCHEMA.to_string(),
            license_assessment: LicenseAssessment {
                schema_version: LWOODZ_LICENSE_SCHEMA.to_string(),
                target: "x".to_string(),
                licences: Vec::new(),
                conflicts: Vec::new(),
                confidence: 1.0,
            },
            provenance_assessment: ProvenanceAssessment {
                schema_version: LWOODZ_PROVENANCE_SCHEMA.to_string(),
                target: "x".to_string(),
                links: Vec::new(),
                uncertain_subjects: Vec::new(),
                confidence: 1.0,
            },
            observations: Vec::new(),
        };
        let decision = LicensePolicy::baseline("MIT").evaluate("x", &evidence);
        assert_eq!(decision.disposition, PolicyDisposition::HumanReview);
    }
}
