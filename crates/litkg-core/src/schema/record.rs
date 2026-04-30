use crate::schema::id::StableId;
use crate::schema::ontology::{EdgeKind, NodeKind};
use crate::schema::provenance::Provenance;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alias {
    pub id: StableId,
    pub label: String,
    pub source: Provenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalNode {
    pub id: StableId,
    pub kind: NodeKind,
    pub label: String,
    pub aliases: Vec<Alias>,
    pub properties: Value,
    pub provenance: Vec<Provenance>,
    pub schema_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalEdge {
    pub id: StableId,
    pub kind: EdgeKind,
    pub source: StableId,
    pub target: StableId,
    pub properties: Value,
    pub evidence: Vec<Provenance>,
    pub confidence: f32,
    pub schema_version: String,
}

impl Alias {
    pub fn validate(&self) -> Result<(), String> {
        self.id.validate()?;
        if self.label.trim().is_empty() {
            return Err(format!("alias {} must have a non-empty label", self.id));
        }
        Ok(())
    }
}

impl CanonicalNode {
    pub fn validate(&self) -> Result<(), String> {
        self.id.validate()?;
        if self.label.trim().is_empty() {
            return Err(format!("node {} must have a non-empty label", self.id));
        }
        if self.provenance.is_empty() {
            return Err(format!(
                "node {} must carry at least one provenance record",
                self.id
            ));
        }
        if self.schema_version.trim().is_empty() {
            return Err(format!(
                "node {} must have a non-empty schema_version",
                self.id
            ));
        }
        for alias in &self.aliases {
            alias.validate()?;
        }
        Ok(())
    }
}

impl CanonicalEdge {
    pub fn validate(&self) -> Result<(), String> {
        self.id.validate()?;
        self.source.validate()?;
        self.target.validate()?;
        if self.evidence.is_empty() {
            return Err(format!(
                "edge {} must carry at least one evidence record",
                self.id
            ));
        }
        if self.schema_version.trim().is_empty() {
            return Err(format!(
                "edge {} must have a non-empty schema_version",
                self.id
            ));
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(format!(
                "edge {} confidence must be between 0 and 1",
                self.id
            ));
        }
        Ok(())
    }
}
