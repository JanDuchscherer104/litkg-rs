use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestEntry {
    pub title: Option<String>,
    pub arxiv_id: String,
    pub tex_dir: String,
    pub source_url: Option<String>,
    pub pdf_url: Option<String>,
    pub pdf_file: Option<String>,
}

impl ManifestEntry {
    pub fn source_url(&self) -> String {
        self.source_url
            .clone()
            .unwrap_or_else(|| format!("https://arxiv.org/e-print/{}", self.arxiv_id))
    }

    pub fn pdf_url(&self) -> Option<String> {
        if let Some(pdf_url) = &self.pdf_url {
            return Some(pdf_url.clone());
        }
        self.pdf_file
            .as_ref()
            .map(|_| format!("https://arxiv.org/pdf/{}.pdf", self.arxiv_id))
    }
}

pub fn load_manifest(path: impl AsRef<Path>) -> Result<Vec<ManifestEntry>> {
    let path = path.as_ref();
    let text = fs::read_to_string(path)
        .with_context(|| format!("Failed to read manifest {}", path.display()))?;
    let mut entries = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let entry: ManifestEntry = serde_json::from_str(trimmed)
            .with_context(|| format!("Invalid JSONL entry at {}:{}", path.display(), index + 1))?;
        entries.push(entry);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_jsonl_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sources.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(
            file,
            "{{\"title\":\"ViSTA\",\"arxiv_id\":\"2509.01584\",\"tex_dir\":\"arxiv-vista\",\"pdf_file\":\"vista.pdf\"}}"
        )
        .unwrap();

        let entries = load_manifest(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].arxiv_id, "2509.01584");
        assert_eq!(
            entries[0].source_url(),
            "https://arxiv.org/e-print/2509.01584"
        );
        assert_eq!(
            entries[0].pdf_url().unwrap(),
            "https://arxiv.org/pdf/2509.01584.pdf"
        );
    }
}
