use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestEntry {
    pub title: Option<String>,
    pub arxiv_id: String,
    pub tex_dir: String,
    #[serde(alias = "url")]
    pub source_url: Option<String>,
    pub pdf_url: Option<String>,
    pub pdf_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relevance_rank: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relevance_category: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub adoptable_ideas: Vec<String>,
}

impl ManifestEntry {
    pub fn source_url(&self) -> Option<String> {
        non_empty(self.source_url.as_deref())
            .map(str::to_string)
            .or_else(|| {
                (!self.arxiv_id.trim().is_empty())
                    .then(|| format!("https://arxiv.org/e-print/{}", self.arxiv_id))
            })
    }

    pub fn pdf_url(&self) -> Option<String> {
        if let Some(pdf_url) = non_empty(self.pdf_url.as_deref()) {
            return Some(pdf_url.to_string());
        }
        if self.arxiv_id.trim().is_empty() {
            return None;
        }
        self.pdf_file()
            .map(|_| format!("https://arxiv.org/pdf/{}.pdf", self.arxiv_id))
    }

    pub fn tex_dir(&self) -> Option<String> {
        non_empty(Some(&self.tex_dir)).map(str::to_string)
    }

    pub fn pdf_file(&self) -> Option<String> {
        non_empty(self.pdf_file.as_deref()).map(str::to_string)
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
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
            entries[0].source_url().as_deref(),
            Some("https://arxiv.org/e-print/2509.01584")
        );
        assert_eq!(
            entries[0].pdf_url().unwrap(),
            "https://arxiv.org/pdf/2509.01584.pdf"
        );
        assert_eq!(entries[0].relevance_rank, None);
        assert!(entries[0].adoptable_ideas.is_empty());
    }

    #[test]
    fn title_only_manifest_entry_has_no_derived_urls() {
        let entry = ManifestEntry {
            title: Some("Local report".into()),
            arxiv_id: String::new(),
            tex_dir: String::new(),
            source_url: None,
            pdf_url: None,
            pdf_file: None,
            relevance_rank: None,
            relevance_category: None,
            adoptable_ideas: Vec::new(),
        };

        assert_eq!(entry.source_url(), None);
        assert_eq!(entry.pdf_url(), None);
    }
}
