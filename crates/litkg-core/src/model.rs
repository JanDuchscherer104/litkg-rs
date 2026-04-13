use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SourceKind {
    Manifest,
    Bib,
    ManifestAndBib,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DownloadMode {
    ManifestSource,
    ManifestSourcePlusPdf,
    MetadataOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ParseStatus {
    PendingDownload,
    Downloaded,
    Parsed,
    MetadataOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperSourceRecord {
    pub paper_id: String,
    pub citation_key: Option<String>,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<String>,
    pub arxiv_id: Option<String>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub tex_dir: Option<String>,
    pub pdf_file: Option<String>,
    pub source_kind: SourceKind,
    pub download_mode: DownloadMode,
    pub has_local_tex: bool,
    pub has_local_pdf: bool,
    pub parse_status: ParseStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperSection {
    pub level: u8,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperFigure {
    pub caption: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperTable {
    pub caption: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParsedPaper {
    pub metadata: PaperSourceRecord,
    pub abstract_text: Option<String>,
    pub sections: Vec<PaperSection>,
    pub figures: Vec<PaperFigure>,
    pub tables: Vec<PaperTable>,
    pub citations: Vec<String>,
    pub provenance: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ResearchMetadata {
    pub venue: Option<String>,
    #[serde(default)]
    pub task_tags: Vec<String>,
    #[serde(default)]
    pub dataset_tags: Vec<String>,
    #[serde(default)]
    pub metric_tags: Vec<String>,
    #[serde(default)]
    pub code_repositories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotebookCellKind {
    Code,
    Markdown,
    Raw,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotebookCell {
    pub cell_index: usize,
    pub cell_kind: NotebookCellKind,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct NotebookDocument {
    pub notebook_id: String,
    pub path: String,
    pub kernel: Option<String>,
    pub language: Option<String>,
    #[serde(default)]
    pub cells: Vec<NotebookCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResearchPaper {
    pub parsed: ParsedPaper,
    #[serde(default)]
    pub research: ResearchMetadata,
    #[serde(default)]
    pub notebooks: Vec<NotebookDocument>,
}

impl ResearchPaper {
    pub fn from_parsed(parsed: ParsedPaper) -> Self {
        Self {
            parsed,
            research: ResearchMetadata::default(),
            notebooks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterializedDoc {
    pub path: PathBuf,
    pub content: String,
}
