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
