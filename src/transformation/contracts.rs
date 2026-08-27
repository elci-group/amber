// Copyright (c) 2024 SVCH <svch@seriousaboutsolutions.co.uk>
// SPDX-License-Identifier: MIT
//! Deserializing adapters for independently versioned lwoodz and Padagonia contracts.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const LWOODZ_EVIDENCE_SCHEMA: &str = "lwoodz.evidence-set/v1";
pub const LWOODZ_LICENSE_SCHEMA: &str = "lwoodz.license-assessment/v1";
pub const LWOODZ_PROVENANCE_SCHEMA: &str = "lwoodz.provenance-assessment/v1";
pub const PADAGONIA_ONTOLOGY_SCHEMA: &str = "padagonia.software-ontology/v1";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceOrigin {
    Declared,
    Observed,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenceFamily {
    Permissive,
    WeakCopyleft,
    StrongCopyleft,
    NetworkCopyleft,
    Proprietary,
    Unknown,
    Conflicting,
    Unverified,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LicenceFact {
    pub subject: String,
    pub expression: String,
    #[serde(default)]
    pub spdx_identifiers: Vec<String>,
    pub family: LicenceFamily,
    pub origin: EvidenceOrigin,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LicensingConflict {
    pub code: String,
    pub subject: String,
    #[serde(default)]
    pub declared_expressions: Vec<String>,
    #[serde(default)]
    pub observed_expressions: Vec<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LicenseAssessment {
    pub schema_version: String,
    pub target: String,
    #[serde(default)]
    pub licences: Vec<LicenceFact>,
    #[serde(default)]
    pub conflicts: Vec<LicensingConflict>,
    pub confidence: f64,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceRelationship {
    DerivesFrom,
    CopiedFrom,
    GeneratedFrom,
    VendoredFrom,
    Wraps,
    Duplicates,
    SimilarTo,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceLink {
    pub subject: String,
    pub source: String,
    pub relationship: ProvenanceRelationship,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProvenanceAssessment {
    pub schema_version: String,
    pub target: String,
    #[serde(default)]
    pub links: Vec<ProvenanceLink>,
    #[serde(default)]
    pub uncertain_subjects: Vec<String>,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LwoodzEvidenceSet {
    pub schema_version: String,
    pub license_assessment: LicenseAssessment,
    pub provenance_assessment: ProvenanceAssessment,
    #[serde(default)]
    pub observations: Vec<serde_json::Value>,
}

impl LwoodzEvidenceSet {
    pub fn from_json(input: &str) -> Result<Self, String> {
        let evidence: Self = serde_json::from_str(input).map_err(|error| error.to_string())?;
        evidence.validate()?;
        Ok(evidence)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != LWOODZ_EVIDENCE_SCHEMA
            || self.license_assessment.schema_version != LWOODZ_LICENSE_SCHEMA
            || self.provenance_assessment.schema_version != LWOODZ_PROVENANCE_SCHEMA
        {
            return Err("unsupported lwoodz contract version".to_string());
        }
        if self.license_assessment.target != self.provenance_assessment.target {
            return Err("lwoodz assessment targets differ".to_string());
        }
        if self
            .license_assessment
            .licences
            .iter()
            .any(|fact| !valid_confidence(fact.confidence))
            || self
                .license_assessment
                .conflicts
                .iter()
                .any(|conflict| !valid_confidence(conflict.confidence))
            || self
                .provenance_assessment
                .links
                .iter()
                .any(|link| !valid_confidence(link.confidence))
        {
            return Err("lwoodz confidence must be between zero and one".to_string());
        }
        Ok(())
    }

    #[must_use]
    pub fn facts_for(&self, subject: &str) -> Vec<&LicenceFact> {
        self.license_assessment
            .licences
            .iter()
            .filter(|fact| fact.subject == subject)
            .collect()
    }

    #[must_use]
    pub fn conflicts_for(&self, subject: &str) -> Vec<&LicensingConflict> {
        self.license_assessment
            .conflicts
            .iter()
            .filter(|conflict| conflict.subject == subject)
            .collect()
    }

    #[must_use]
    pub fn has_issue_below(&self, subject: &str) -> bool {
        let prefix = format!("{subject}/");
        self.license_assessment
            .conflicts
            .iter()
            .any(|conflict| conflict.subject.starts_with(&prefix))
            || self
                .provenance_assessment
                .uncertain_subjects
                .iter()
                .any(|item| item.starts_with(&prefix))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SoftwareRelationshipKind {
    Contains,
    DependsOn,
    Implements,
    Wraps,
    DerivesFrom,
    Calls,
    Generates,
    Adapts,
    Embeds,
    Duplicates,
    InterfacesWith,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransformationBoundary {
    pub independently_addressable: bool,
    pub independently_transformable: bool,
    pub boundary_established: bool,
    pub granularity: u32,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoftwareComponent {
    pub id: String,
    pub name: String,
    pub kind: serde_json::Value,
    pub path: Option<String>,
    pub boundary: TransformationBoundary,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoftwareRelationship {
    pub source: String,
    pub target: String,
    pub kind: SoftwareRelationshipKind,
    pub confidence: f64,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RelationshipSet {
    #[serde(default)]
    pub relationships: Vec<SoftwareRelationship>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecompositionConfidence {
    pub overall: f64,
    #[serde(default)]
    pub component_boundaries: BTreeMap<String, f64>,
    #[serde(default)]
    pub unresolved_boundaries: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentGraph {
    pub root: String,
    pub components: Vec<SoftwareComponent>,
    pub relationships: RelationshipSet,
    pub decomposition_confidence: DecompositionConfidence,
}

impl ComponentGraph {
    #[must_use]
    pub fn component(&self, id: &str) -> Option<&SoftwareComponent> {
        self.components.iter().find(|component| component.id == id)
    }

    #[must_use]
    pub fn child_ids(&self, id: &str) -> Vec<String> {
        self.relationships
            .relationships
            .iter()
            .filter(|relationship| {
                relationship.source == id && relationship.kind == SoftwareRelationshipKind::Contains
            })
            .map(|relationship| relationship.target.clone())
            .collect()
    }

    #[must_use]
    pub fn boundary_confidence(&self, id: &str) -> f64 {
        self.decomposition_confidence
            .component_boundaries
            .get(id)
            .copied()
            .unwrap_or(self.decomposition_confidence.overall)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PadagoniaOntology {
    pub schema_version: String,
    pub namespace: String,
    pub graph: ComponentGraph,
    #[serde(default)]
    pub vocabulary: BTreeMap<String, String>,
}

impl PadagoniaOntology {
    pub fn from_json(input: &str) -> Result<Self, String> {
        let ontology: Self = serde_json::from_str(input).map_err(|error| error.to_string())?;
        ontology.validate()?;
        Ok(ontology)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != PADAGONIA_ONTOLOGY_SCHEMA {
            return Err("unsupported Padagonia contract version".to_string());
        }
        let ids: BTreeSet<&str> = self
            .graph
            .components
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        if !ids.contains(self.graph.root.as_str()) || ids.len() != self.graph.components.len() {
            return Err("Padagonia graph has a missing root or duplicate component".to_string());
        }
        if self
            .graph
            .relationships
            .relationships
            .iter()
            .any(|relationship| {
                !ids.contains(relationship.source.as_str())
                    || !ids.contains(relationship.target.as_str())
                    || !valid_confidence(relationship.confidence)
            })
        {
            return Err("Padagonia graph has an invalid relationship".to_string());
        }
        Ok(())
    }
}

const fn valid_confidence(value: f64) -> bool {
    value.is_finite() && value >= 0.0 && value <= 1.0
}
