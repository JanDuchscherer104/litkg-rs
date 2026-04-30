use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceSpan {
    ByteRange {
        start: usize,
        end: usize,
    },
    LineRange {
        start: usize,
        end: usize,
    },
    SectionAnchor {
        path: String,
        anchor: String,
    },
    UrlFragment {
        url: String,
        fragment: Option<String>,
    },
    TranscriptTurn {
        session_id: String,
        turn_id: String,
    },
    ApiField {
        provider: String,
        field_path: String,
    },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub source_id: String,
    pub source_hash: String,
    pub span: ProvenanceSpan,
    pub adapter_name: String,
    pub adapter_version: String,
    pub ingested_at: String, // ISO 8601
    pub confidence: f32,
}
