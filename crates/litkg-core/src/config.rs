use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SinkMode {
    Graphify,
    Neo4j,
    Both,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepoConfig {
    pub manifest_path: PathBuf,
    pub bib_path: PathBuf,
    pub tex_root: PathBuf,
    pub pdf_root: PathBuf,
    pub generated_docs_root: PathBuf,
    pub registry_path: Option<PathBuf>,
    pub parsed_root: Option<PathBuf>,
    pub neo4j_export_root: Option<PathBuf>,
    #[serde(default)]
    pub memory_state_root: Option<PathBuf>,
    pub sink: SinkMode,
    pub graphify_rebuild_command: Option<String>,
    #[serde(default)]
    pub download_pdfs: bool,
    #[serde(default)]
    pub relevance_tags: Vec<String>,
    #[serde(default)]
    pub semantic_scholar: Option<SemanticScholarConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SemanticScholarConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_semantic_scholar_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_semantic_scholar_min_interval_s")]
    pub min_interval_s: f64,
    #[serde(default = "default_semantic_scholar_max_retries")]
    pub max_retries: usize,
    #[serde(default = "default_semantic_scholar_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_semantic_scholar_fields")]
    pub fields: Vec<String>,
}

impl Default for SemanticScholarConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key_env: default_semantic_scholar_api_key_env(),
            min_interval_s: default_semantic_scholar_min_interval_s(),
            max_retries: default_semantic_scholar_max_retries(),
            batch_size: default_semantic_scholar_batch_size(),
            fields: default_semantic_scholar_fields(),
        }
    }
}

impl RepoConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config {}", path.display()))?;
        let config: Self = toml::from_str(&raw)
            .with_context(|| format!("Failed to parse config {}", path.display()))?;
        Ok(config)
    }

    pub fn registry_path(&self) -> PathBuf {
        self.registry_path
            .clone()
            .unwrap_or_else(|| self.generated_docs_root.join("registry.jsonl"))
    }

    pub fn parsed_root(&self) -> PathBuf {
        self.parsed_root
            .clone()
            .unwrap_or_else(|| self.generated_docs_root.join("parsed"))
    }

    pub fn neo4j_export_root(&self) -> PathBuf {
        self.neo4j_export_root
            .clone()
            .unwrap_or_else(|| self.generated_docs_root.join("neo4j-export"))
    }

    pub fn memory_state_root(&self) -> Option<PathBuf> {
        self.memory_state_root.clone()
    }

    pub fn semantic_scholar_config(&self) -> SemanticScholarConfig {
        self.semantic_scholar.clone().unwrap_or_default()
    }
}

fn default_semantic_scholar_api_key_env() -> String {
    "SEMANTIC_SCHOLAR_API_KEY".into()
}

fn default_semantic_scholar_min_interval_s() -> f64 {
    1.05
}

fn default_semantic_scholar_max_retries() -> usize {
    4
}

fn default_semantic_scholar_batch_size() -> usize {
    100
}

pub fn default_semantic_scholar_fields() -> Vec<String> {
    [
        "paperId",
        "corpusId",
        "externalIds",
        "url",
        "title",
        "abstract",
        "tldr",
        "venue",
        "year",
        "publicationDate",
        "publicationTypes",
        "fieldsOfStudy",
        "s2FieldsOfStudy",
        "authors",
        "citationCount",
        "influentialCitationCount",
        "referenceCount",
        "openAccessPdf",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}
