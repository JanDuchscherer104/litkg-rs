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
    pub sink: SinkMode,
    pub graphify_rebuild_command: Option<String>,
    #[serde(default)]
    pub download_pdfs: bool,
    #[serde(default)]
    pub relevance_tags: Vec<String>,
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
}
