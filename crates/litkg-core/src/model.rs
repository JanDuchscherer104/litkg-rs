use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_scholar: Option<SemanticScholarPaper>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SemanticScholarPaper {
    pub paper_id: Option<String>,
    pub corpus_id: Option<u64>,
    #[serde(
        default,
        deserialize_with = "deserialize_semantic_scholar_external_ids",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub external_ids: BTreeMap<String, String>,
    pub url: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "abstract")]
    pub abstract_text: Option<String>,
    pub tldr: Option<SemanticScholarTldr>,
    pub venue: Option<String>,
    pub year: Option<i32>,
    pub publication_date: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_semantic_scholar_vec",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub publication_types: Vec<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_semantic_scholar_vec",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub fields_of_study: Vec<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_semantic_scholar_vec",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub s2_fields_of_study: Vec<SemanticScholarFieldOfStudy>,
    #[serde(
        default,
        deserialize_with = "deserialize_semantic_scholar_vec",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub authors: Vec<SemanticScholarAuthor>,
    pub citation_count: Option<u64>,
    pub influential_citation_count: Option<u64>,
    pub reference_count: Option<u64>,
    pub open_access_pdf: Option<SemanticScholarOpenAccessPdf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SemanticScholarAuthor {
    pub author_id: Option<String>,
    pub name: String,
    pub url: Option<String>,
    pub paper_count: Option<u64>,
    pub citation_count: Option<u64>,
    pub h_index: Option<u64>,
    #[serde(
        default,
        deserialize_with = "deserialize_semantic_scholar_vec",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub affiliations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SemanticScholarTldr {
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SemanticScholarOpenAccessPdf {
    pub url: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SemanticScholarFieldOfStudy {
    pub category: Option<String>,
    pub source: Option<String>,
}

fn deserialize_semantic_scholar_external_ids<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Option::<BTreeMap<String, Value>>::deserialize(deserializer)?.unwrap_or_default();
    Ok(raw
        .into_iter()
        .filter_map(|(key, value)| match value {
            Value::String(text) => Some((key, text)),
            Value::Number(number) => Some((key, number.to_string())),
            Value::Bool(flag) => Some((key, flag.to_string())),
            _ => None,
        })
        .collect())
}

fn deserialize_semantic_scholar_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CitationReference {
    pub key: String,
    pub title: Option<String>,
    pub doi: Option<String>,
    pub arxiv_id: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParsedPaper {
    pub metadata: PaperSourceRecord,
    pub abstract_text: Option<String>,
    pub sections: Vec<PaperSection>,
    pub figures: Vec<PaperFigure>,
    pub tables: Vec<PaperTable>,
    pub citations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citation_references: Vec<CitationReference>,
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
