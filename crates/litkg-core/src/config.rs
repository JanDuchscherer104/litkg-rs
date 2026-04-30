use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
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

    // New sections from expanded toml
    #[serde(default)]
    pub project: Option<ProjectConfig>,
    #[serde(default)]
    pub sources: BTreeMap<String, SourceConfig>,
    #[serde(default)]
    pub representation: Option<RepresentationConfig>,
    #[serde(default)]
    pub backends: Option<BackendsConfig>,
    #[serde(default)]
    pub storage: Option<StorageConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ProjectConfig {
    pub id: String,
    pub name: String,
    pub root: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SourceConfig {
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub symbols: bool,
    #[serde(default)]
    pub edges: Option<String>,
    #[serde(default)]
    pub entrypoints: Vec<PathBuf>,
    #[serde(default)]
    pub manifest: Option<PathBuf>,
    #[serde(default)]
    pub bib: Option<PathBuf>,
    #[serde(default)]
    pub pdfs: Option<String>,
    #[serde(default)]
    pub tex: Option<String>,
    #[serde(default)]
    pub context7_libraries: Vec<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub markitdown: bool,
    #[serde(default)]
    pub urls: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RepresentationConfig {
    pub primary: String,
    #[serde(default)]
    pub durable_exports: Vec<String>,
    #[serde(default)]
    pub runtime_enrichment: Vec<String>,
    #[serde(default)]
    pub optional_runtime: Vec<String>,
    pub memory_backend: Option<String>,
    pub memory_backend_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BackendsConfig {
    #[serde(default)]
    pub graphify: bool,
    #[serde(default)]
    pub neo4j_export: bool,
    #[serde(default)]
    pub code_index: bool,
    #[serde(default)]
    pub graphiti: bool,
    #[serde(default)]
    pub mempalace: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StorageConfig {
    pub generated_root: PathBuf,
    pub db_root: PathBuf,
    pub runtime_cache_root: PathBuf,
    #[serde(default)]
    pub lfs: bool,
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
        if let Some(path) = &self.registry_path {
            return path.clone();
        }
        if let Some(storage) = &self.storage {
            return storage.generated_root.join("literature/registry.jsonl");
        }
        self.generated_docs_root.join("registry.jsonl")
    }

    pub fn parsed_root(&self) -> PathBuf {
        if let Some(path) = &self.parsed_root {
            return path.clone();
        }
        if let Some(storage) = &self.storage {
            return storage.generated_root.join("literature/parsed");
        }
        self.generated_docs_root.join("parsed")
    }

    pub fn neo4j_export_root(&self) -> PathBuf {
        if let Some(path) = &self.neo4j_export_root {
            return path.clone();
        }
        if let Some(storage) = &self.storage {
            return storage.generated_root.join("neo4j-export");
        }
        self.generated_docs_root.join("neo4j-export")
    }

    pub fn memory_state_root(&self) -> Option<PathBuf> {
        if let Some(path) = &self.memory_state_root {
            return Some(path.clone());
        }
        if let Some(storage) = &self.storage {
            return Some(storage.db_root.join("memory"));
        }
        None
    }

    pub fn runtime_cache_root(&self) -> PathBuf {
        if let Some(storage) = &self.storage {
            return storage.runtime_cache_root.clone();
        }
        PathBuf::from(".cache/kg")
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
