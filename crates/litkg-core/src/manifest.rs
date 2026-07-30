use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestEntry {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub arxiv_id: String,
    #[serde(default)]
    pub tex_dir: String,
    #[serde(default, alias = "url")]
    pub source_url: Option<String>,
    #[serde(default)]
    pub pdf_url: Option<String>,
    #[serde(default)]
    pub pdf_file: Option<String>,
}

impl ManifestEntry {
    pub fn arxiv_id(&self) -> Option<&str> {
        non_empty(&self.arxiv_id)
    }

    pub fn tex_dir(&self) -> Option<&str> {
        non_empty(&self.tex_dir)
    }

    pub fn source_url(&self) -> Option<String> {
        self.source_url.clone().or_else(|| {
            self.arxiv_id()
                .map(|arxiv_id| format!("https://arxiv.org/e-print/{arxiv_id}"))
        })
    }

    pub fn pdf_url(&self) -> Option<String> {
        self.pdf_url.clone().or_else(|| {
            self.pdf_file.as_ref().and_then(|_| {
                self.arxiv_id()
                    .map(|arxiv_id| format!("https://arxiv.org/pdf/{arxiv_id}.pdf"))
            })
        })
    }

    pub fn identity(&self) -> String {
        if let Some(arxiv_id) = self.arxiv_id() {
            return format!("arxiv:{arxiv_id}");
        }
        if let Some(title) = self
            .title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
        {
            return format!("title:{}", normalize_identity(title));
        }
        if let Some(url) = self
            .pdf_url
            .as_deref()
            .or(self.source_url.as_deref())
            .filter(|url| !url.trim().is_empty())
        {
            return format!("url:{url}");
        }
        format!(
            "path:{}:{}",
            self.tex_dir,
            self.pdf_file.as_deref().unwrap_or_default()
        )
    }
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn normalize_identity(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
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
    }

    #[test]
    fn accepts_pdf_only_https_source() {
        let entry: ManifestEntry = serde_json::from_str(
            r#"{"title":"Example Book","url":"https://example.org/book","pdf_url":"https://example.org/book.pdf","pdf_file":"book.pdf"}"#,
        )
        .unwrap();

        assert_eq!(entry.arxiv_id(), None);
        assert_eq!(entry.tex_dir(), None);
        assert_eq!(
            entry.source_url().as_deref(),
            Some("https://example.org/book")
        );
        assert_eq!(
            entry.pdf_url().as_deref(),
            Some("https://example.org/book.pdf")
        );
        assert_eq!(entry.identity(), "title:examplebook");
    }
}
