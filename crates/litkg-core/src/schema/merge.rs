use crate::schema::id::StableId;
use crate::schema::provenance::Provenance;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeReason {
    DoiExact,
    ArxivExact,
    S2PaperIdExact,
    CorpusIdExact,
    TitleAuthorYearExact,
    TitleAuthorYearFuzzy,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergeDecision {
    pub canonical_id: StableId,
    pub merged_id: StableId,
    pub reason: MergeReason,
    pub confidence: f32,
    pub evidence: Vec<Provenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    DoiTitleMismatch,
    BibKeyDoiMismatch,
    TitleYearMismatchAuthor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    pub id: StableId,
    pub kind: ConflictKind,
    pub nodes: Vec<StableId>,
    pub message: String,
    pub evidence: Vec<Provenance>,
}
