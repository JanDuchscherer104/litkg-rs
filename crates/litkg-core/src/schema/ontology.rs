use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Paper,
    BibEntry,
    Document,
    DocSection,
    CitationMention,
    Concept,
    Decision,
    OpenQuestion,
    ActionItem,
    CodeSymbol,
    Transcript,
    TranscriptTurn,
    Context7Leaf,
    Conflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Cites,
    Mentions,
    ResolvesTo,
    CanonicalizesTo,
    Aliases,
    Defines,
    Documents,
    Implements,
    Explains,
    Supports,
    Contradicts,
    Supersedes,
    DependsOn,
    RequiresExternalDocs,
    HasEvidence,
}
