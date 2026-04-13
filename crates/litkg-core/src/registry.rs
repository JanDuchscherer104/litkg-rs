use crate::bibtex::{parse_bibtex, BibEntry};
use crate::config::RepoConfig;
use crate::manifest::{load_manifest, ManifestEntry};
use crate::model::{DownloadMode, PaperSourceRecord, ParseStatus, SourceKind};
use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub fn sync_registry(config: &RepoConfig) -> Result<Vec<PaperSourceRecord>> {
    let registry = build_registry_snapshot(config)?;
    write_registry(&config.registry_path(), &registry)?;
    Ok(registry)
}

pub fn build_registry_snapshot(config: &RepoConfig) -> Result<Vec<PaperSourceRecord>> {
    let manifest = load_manifest(&config.manifest_path)?;
    let bib_text = fs::read_to_string(&config.bib_path)
        .with_context(|| format!("Failed to read bibliography {}", config.bib_path.display()))?;
    let bib_entries = parse_bibtex(&bib_text)?;
    Ok(merge_registry(manifest, bib_entries, config))
}

pub fn write_registry(path: &Path, registry: &[PaperSourceRecord]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create registry directory {}", parent.display()))?;
    }
    let mut lines = Vec::with_capacity(registry.len());
    for record in registry {
        lines.push(serde_json::to_string(record)?);
    }
    fs::write(path, lines.join("\n") + "\n")
        .with_context(|| format!("Failed to write registry {}", path.display()))?;
    Ok(())
}

pub fn load_registry(path: impl AsRef<Path>) -> Result<Vec<PaperSourceRecord>> {
    let path = path.as_ref();
    let text = fs::read_to_string(path)
        .with_context(|| format!("Failed to read registry {}", path.display()))?;
    let mut records = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        records.push(
            serde_json::from_str(line)
                .with_context(|| format!("Invalid registry row in {}", path.display()))?,
        );
    }
    Ok(records)
}

pub fn merge_registry(
    manifest_entries: Vec<ManifestEntry>,
    bib_entries: Vec<BibEntry>,
    config: &RepoConfig,
) -> Vec<PaperSourceRecord> {
    let manifest_by_arxiv: BTreeMap<String, ManifestEntry> = manifest_entries
        .into_iter()
        .map(|entry| (entry.arxiv_id.clone(), entry))
        .collect();
    let mut used_manifest_ids = BTreeSet::new();
    let mut registry = Vec::new();

    for bib in &bib_entries {
        let bib_arxiv = bib.fields.get("eprint").cloned();
        let matched_manifest = bib_arxiv
            .as_ref()
            .and_then(|arxiv_id| manifest_by_arxiv.get(arxiv_id))
            .cloned()
            .or_else(|| {
                manifest_by_arxiv
                    .values()
                    .find(|entry| {
                        normalize_title(entry.title.as_deref().unwrap_or_default())
                            == normalize_title(
                                bib.fields
                                    .get("title")
                                    .map(String::as_str)
                                    .unwrap_or_default(),
                            )
                    })
                    .cloned()
            });

        if let Some(manifest) = &matched_manifest {
            used_manifest_ids.insert(manifest.arxiv_id.clone());
        }

        registry.push(build_record_from_bib_and_manifest(
            bib,
            matched_manifest.as_ref(),
            config,
        ));
    }

    for manifest in manifest_by_arxiv.values() {
        if used_manifest_ids.contains(&manifest.arxiv_id) {
            continue;
        }
        registry.push(build_record_from_manifest_only(manifest, config));
    }

    registry.sort_by(|left, right| left.paper_id.cmp(&right.paper_id));
    registry
}

fn build_record_from_bib_and_manifest(
    bib: &BibEntry,
    manifest: Option<&ManifestEntry>,
    config: &RepoConfig,
) -> PaperSourceRecord {
    let title = bib
        .fields
        .get("title")
        .cloned()
        .or_else(|| manifest.and_then(|item| item.title.clone()))
        .unwrap_or_else(|| bib.citation_key.clone());
    let arxiv_id = manifest
        .map(|item| item.arxiv_id.clone())
        .or_else(|| bib.fields.get("eprint").cloned());
    let doi = bib.fields.get("doi").cloned();
    let url = bib
        .fields
        .get("url")
        .cloned()
        .or_else(|| manifest.map(|item| item.source_url()));
    let tex_dir = manifest.map(|item| item.tex_dir.clone());
    let pdf_file = manifest.and_then(|item| item.pdf_file.clone());
    let has_local_tex = tex_dir
        .as_ref()
        .map(|dir| path_has_files(&config.tex_root.join(dir)))
        .unwrap_or(false);
    let has_local_pdf = pdf_file
        .as_ref()
        .map(|file| config.pdf_root.join(file).is_file())
        .unwrap_or(false);

    PaperSourceRecord {
        paper_id: make_paper_id(Some(&bib.citation_key), arxiv_id.as_deref(), &title),
        citation_key: Some(bib.citation_key.clone()),
        title,
        authors: split_authors(bib.fields.get("author")),
        year: bib.fields.get("year").cloned(),
        arxiv_id,
        doi,
        url,
        tex_dir,
        pdf_file: pdf_file.clone(),
        source_kind: if manifest.is_some() {
            SourceKind::ManifestAndBib
        } else {
            SourceKind::Bib
        },
        download_mode: if manifest.is_some() {
            if pdf_file.is_some() {
                DownloadMode::ManifestSourcePlusPdf
            } else {
                DownloadMode::ManifestSource
            }
        } else {
            DownloadMode::MetadataOnly
        },
        has_local_tex,
        has_local_pdf,
        parse_status: if manifest.is_none() {
            ParseStatus::MetadataOnly
        } else if has_local_tex {
            ParseStatus::Downloaded
        } else {
            ParseStatus::PendingDownload
        },
    }
}

fn build_record_from_manifest_only(
    manifest: &ManifestEntry,
    config: &RepoConfig,
) -> PaperSourceRecord {
    let has_local_tex = path_has_files(&config.tex_root.join(&manifest.tex_dir));
    let has_local_pdf = manifest
        .pdf_file
        .as_ref()
        .map(|file| config.pdf_root.join(file).is_file())
        .unwrap_or(false);

    PaperSourceRecord {
        paper_id: make_paper_id(
            None,
            Some(&manifest.arxiv_id),
            manifest.title.as_deref().unwrap_or(&manifest.tex_dir),
        ),
        citation_key: None,
        title: manifest
            .title
            .clone()
            .unwrap_or_else(|| manifest.tex_dir.clone()),
        authors: Vec::new(),
        year: None,
        arxiv_id: Some(manifest.arxiv_id.clone()),
        doi: None,
        url: manifest.pdf_url().or_else(|| Some(manifest.source_url())),
        tex_dir: Some(manifest.tex_dir.clone()),
        pdf_file: manifest.pdf_file.clone(),
        source_kind: SourceKind::Manifest,
        download_mode: if manifest.pdf_file.is_some() {
            DownloadMode::ManifestSourcePlusPdf
        } else {
            DownloadMode::ManifestSource
        },
        has_local_tex,
        has_local_pdf,
        parse_status: if has_local_tex {
            ParseStatus::Downloaded
        } else {
            ParseStatus::PendingDownload
        },
    }
}

fn split_authors(field: Option<&String>) -> Vec<String> {
    field
        .map(|value| {
            value
                .split(" and ")
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn make_paper_id(citation_key: Option<&str>, arxiv_id: Option<&str>, title: &str) -> String {
    if let Some(key) = citation_key {
        return slugify(key);
    }
    if let Some(arxiv_id) = arxiv_id {
        return format!("arxiv-{}", slugify(arxiv_id));
    }
    slugify(title)
}

fn slugify(text: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn normalize_title(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn path_has_files(path: &Path) -> bool {
    path.is_dir()
        && path
            .read_dir()
            .map(|entries| {
                entries.flatten().any(|entry| {
                    let path = entry.path();
                    path.is_file() || path_has_files(&path)
                })
            })
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SinkMode;

    fn sample_config(root: &Path) -> RepoConfig {
        RepoConfig {
            manifest_path: root.join("sources.jsonl"),
            bib_path: root.join("references.bib"),
            tex_root: root.join("tex"),
            pdf_root: root.join("pdf"),
            generated_docs_root: root.join("generated"),
            registry_path: None,
            parsed_root: None,
            neo4j_export_root: None,
            sink: SinkMode::Graphify,
            graphify_rebuild_command: None,
            download_pdfs: true,
            relevance_tags: vec![],
        }
    }

    #[test]
    fn merges_manifest_and_bib_by_arxiv() {
        let dir = tempfile::tempdir().unwrap();
        let config = sample_config(dir.path());
        let manifest = vec![ManifestEntry {
            title: Some("ViSTA-SLAM".into()),
            arxiv_id: "2509.01584".into(),
            tex_dir: "vista".into(),
            source_url: None,
            pdf_url: None,
            pdf_file: Some("vista.pdf".into()),
        }];
        let bib = vec![BibEntry {
            entry_type: "misc".into(),
            citation_key: "zhang2026vistaslam".into(),
            fields: BTreeMap::from([
                ("title".into(), "ViSTA-SLAM".into()),
                ("author".into(), "Ganlin Zhang and Shenhan Qian".into()),
                ("year".into(), "2026".into()),
                ("eprint".into(), "2509.01584".into()),
            ]),
        }];

        let registry = merge_registry(manifest, bib, &config);
        assert_eq!(registry.len(), 1);
        assert_eq!(registry[0].source_kind, SourceKind::ManifestAndBib);
        assert_eq!(registry[0].authors.len(), 2);
        assert_eq!(
            registry[0].download_mode,
            DownloadMode::ManifestSourcePlusPdf
        );
    }
}
