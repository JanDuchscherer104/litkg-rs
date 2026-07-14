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
    let manifest_by_key: BTreeMap<String, ManifestEntry> = manifest_entries
        .into_iter()
        .enumerate()
        .map(|(index, entry)| (manifest_entry_key(index, &entry), entry))
        .collect();
    let mut manifest_by_normalized_title: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (manifest_key, entry) in &manifest_by_key {
        if let Some(title) = entry.title.as_deref() {
            let normalized = normalize_title(title);
            if !normalized.is_empty() {
                manifest_by_normalized_title
                    .entry(normalized)
                    .or_default()
                    .push(manifest_key.clone());
            }
        }
    }
    let mut used_manifest_ids = BTreeSet::new();
    let mut registry = Vec::new();

    for bib in &bib_entries {
        let bib_arxiv = bib.fields.get("eprint").cloned();
        let bib_title_normalized = normalize_title(
            bib.fields
                .get("title")
                .map(String::as_str)
                .unwrap_or_default(),
        );
        let matched_manifest = bib_arxiv
            .as_ref()
            .filter(|arxiv_id| !arxiv_id.trim().is_empty())
            .and_then(|arxiv_id| {
                manifest_by_key
                    .get(arxiv_id)
                    .map(|entry| (arxiv_id.clone(), entry.clone()))
            })
            .or_else(|| {
                if bib_title_normalized.is_empty() {
                    return None;
                }
                manifest_by_normalized_title
                    .get(&bib_title_normalized)
                    .and_then(|candidate_ids| {
                        candidate_ids
                            .iter()
                            .find(|candidate_id| !used_manifest_ids.contains(*candidate_id))
                            .or_else(|| candidate_ids.first())
                    })
                    .and_then(|candidate_id| {
                        manifest_by_key
                            .get(candidate_id)
                            .map(|entry| (candidate_id.clone(), entry.clone()))
                    })
            });

        if let Some((manifest_key, _)) = &matched_manifest {
            used_manifest_ids.insert(manifest_key.clone());
        }

        let matched_manifest_ref = matched_manifest.as_ref().map(|(_, manifest)| manifest);
        registry.push(build_record_from_bib_and_manifest(
            bib,
            matched_manifest_ref,
            config,
        ));
    }

    for (manifest_key, manifest) in &manifest_by_key {
        if used_manifest_ids.contains(manifest_key) {
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
        .and_then(|item| (!item.arxiv_id.trim().is_empty()).then_some(item.arxiv_id.clone()))
        .or_else(|| bib.fields.get("eprint").cloned());
    let doi = bib.fields.get("doi").cloned();
    let url = bib
        .fields
        .get("url")
        .cloned()
        .or_else(|| manifest.and_then(ManifestEntry::source_url));
    let tex_dir = manifest.and_then(ManifestEntry::tex_dir);
    let pdf_file = manifest.and_then(ManifestEntry::pdf_file);
    let has_local_tex = tex_dir
        .as_ref()
        .map(|dir| path_has_files(&config.tex_root.join(dir)))
        .unwrap_or(false);
    let has_local_pdf = pdf_file
        .as_ref()
        .map(|file| config.pdf_root.join(file).is_file())
        .unwrap_or(false);
    let download_mode = if manifest.is_some() {
        if pdf_file.is_some() {
            DownloadMode::ManifestSourcePlusPdf
        } else if tex_dir.is_some() {
            DownloadMode::ManifestSource
        } else {
            DownloadMode::MetadataOnly
        }
    } else {
        DownloadMode::MetadataOnly
    };
    let parse_status = if manifest.is_none() || (tex_dir.is_none() && pdf_file.is_none()) {
        ParseStatus::MetadataOnly
    } else if has_local_tex {
        ParseStatus::Downloaded
    } else {
        ParseStatus::PendingDownload
    };

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
        download_mode,
        has_local_tex,
        has_local_pdf,
        parse_status,
        relevance_rank: manifest.and_then(|item| item.relevance_rank),
        relevance_category: manifest.and_then(|item| item.relevance_category.clone()),
        adoptable_ideas: manifest
            .map(|item| item.adoptable_ideas.clone())
            .unwrap_or_default(),
        semantic_scholar: None,
    }
}

fn build_record_from_manifest_only(
    manifest: &ManifestEntry,
    config: &RepoConfig,
) -> PaperSourceRecord {
    let tex_dir = manifest.tex_dir();
    let pdf_file = manifest.pdf_file();
    let has_local_tex = tex_dir
        .as_ref()
        .map(|dir| path_has_files(&config.tex_root.join(dir)))
        .unwrap_or(false);
    let has_local_pdf = pdf_file
        .as_ref()
        .map(|file| config.pdf_root.join(file).is_file())
        .unwrap_or(false);

    PaperSourceRecord {
        paper_id: make_paper_id(
            None,
            (!manifest.arxiv_id.trim().is_empty()).then_some(manifest.arxiv_id.as_str()),
            manifest.title.as_deref().unwrap_or(&manifest.tex_dir),
        ),
        citation_key: None,
        title: manifest
            .title
            .clone()
            .unwrap_or_else(|| manifest.tex_dir.clone()),
        authors: Vec::new(),
        year: None,
        arxiv_id: (!manifest.arxiv_id.trim().is_empty()).then_some(manifest.arxiv_id.clone()),
        doi: None,
        url: manifest.pdf_url().or_else(|| manifest.source_url()),
        tex_dir,
        pdf_file: pdf_file.clone(),
        source_kind: SourceKind::Manifest,
        download_mode: if pdf_file.is_some() {
            DownloadMode::ManifestSourcePlusPdf
        } else if manifest.tex_dir().is_none() {
            DownloadMode::MetadataOnly
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
        relevance_rank: manifest.relevance_rank,
        relevance_category: manifest.relevance_category.clone(),
        adoptable_ideas: manifest.adoptable_ideas.clone(),
        semantic_scholar: None,
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

fn manifest_entry_key(index: usize, entry: &ManifestEntry) -> String {
    if !entry.arxiv_id.trim().is_empty() {
        return entry.arxiv_id.clone();
    }
    let label = entry
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(&entry.tex_dir);
    format!("manifest-{index}-{}", slugify(label))
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
            project: None,
            sources: std::collections::BTreeMap::new(),
            representation: None,
            backends: None,
            storage: None,
            manifest_path: root.join("sources.jsonl"),
            bib_path: root.join("references.bib"),
            tex_root: root.join("tex"),
            pdf_root: root.join("pdf"),
            generated_docs_root: root.join("generated"),
            registry_path: None,
            parsed_root: None,
            neo4j_export_root: None,
            memory_state_root: None,
            sink: SinkMode::Graphify,
            graphify_rebuild_command: None,
            download_pdfs: true,
            relevance_tags: vec![],
            semantic_scholar: None,
            authority_tiers: None,
            context_pack: Default::default(),
            runtime: None,
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
            relevance_rank: Some(5),
            relevance_category: Some("visual SLAM benchmark".into()),
            adoptable_ideas: vec!["exercise manifest relevance propagation".into()],
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
        assert_eq!(registry[0].relevance_rank, Some(5));
        assert_eq!(
            registry[0].relevance_category.as_deref(),
            Some("visual SLAM benchmark")
        );
        assert_eq!(registry[0].adoptable_ideas.len(), 1);
    }

    #[test]
    fn merges_manifest_and_bib_by_normalized_title_without_eprint() {
        let dir = tempfile::tempdir().unwrap();
        let config = sample_config(dir.path());
        let manifest = vec![ManifestEntry {
            title: Some("ViSTA-SLAM: Visual-Text SLAM".into()),
            arxiv_id: "2509.01584".into(),
            tex_dir: "vista".into(),
            source_url: None,
            pdf_url: None,
            pdf_file: None,
            relevance_rank: None,
            relevance_category: None,
            adoptable_ideas: Vec::new(),
        }];
        let bib = vec![BibEntry {
            entry_type: "misc".into(),
            citation_key: "zhang2026vistaslam".into(),
            fields: BTreeMap::from([
                ("title".into(), "vista slam visual text slam".into()),
                ("author".into(), "Ganlin Zhang".into()),
                ("year".into(), "2026".into()),
            ]),
        }];

        let registry = merge_registry(manifest, bib, &config);
        assert_eq!(registry.len(), 1);
        assert_eq!(registry[0].source_kind, SourceKind::ManifestAndBib);
        assert_eq!(registry[0].arxiv_id.as_deref(), Some("2509.01584"));
    }

    #[test]
    fn merges_large_title_only_surface_without_duplicate_manifest_use() {
        let dir = tempfile::tempdir().unwrap();
        let config = sample_config(dir.path());

        let mut manifest = Vec::new();
        for idx in 0..400usize {
            manifest.push(ManifestEntry {
                title: Some(format!("Synthetic Vision Paper {idx}")),
                arxiv_id: format!("2601.{idx:05}"),
                tex_dir: format!("paper-{idx}"),
                source_url: None,
                pdf_url: None,
                pdf_file: None,
                relevance_rank: None,
                relevance_category: None,
                adoptable_ideas: Vec::new(),
            });
        }

        let bib = vec![BibEntry {
            entry_type: "misc".into(),
            citation_key: "target-paper".into(),
            fields: BTreeMap::from([
                ("title".into(), "SYNTHETIC VISION PAPER 233!!!".into()),
                ("author".into(), "Ada Lovelace".into()),
                ("year".into(), "2026".into()),
            ]),
        }];

        let registry = merge_registry(manifest, bib, &config);
        let target = registry
            .iter()
            .find(|record| record.citation_key.as_deref() == Some("target-paper"))
            .unwrap();

        assert_eq!(target.source_kind, SourceKind::ManifestAndBib);
        assert_eq!(target.arxiv_id.as_deref(), Some("2601.00233"));
        assert_eq!(registry.len(), 400);
        assert_eq!(
            registry
                .iter()
                .filter(|record| record.source_kind == SourceKind::Manifest)
                .count(),
            399
        );
    }

    #[test]
    fn preserves_manifest_entries_without_arxiv_ids() {
        let dir = tempfile::tempdir().unwrap();
        let config = sample_config(dir.path());
        let manifest = vec![
            ManifestEntry {
                title: Some("First Web Source".into()),
                arxiv_id: String::new(),
                tex_dir: String::new(),
                source_url: Some("https://example.test/first".into()),
                pdf_url: None,
                pdf_file: None,
                relevance_rank: Some(1),
                relevance_category: Some("first category".into()),
                adoptable_ideas: vec!["first idea".into()],
            },
            ManifestEntry {
                title: Some("Second Web Source".into()),
                arxiv_id: String::new(),
                tex_dir: String::new(),
                source_url: Some("https://example.test/second".into()),
                pdf_url: None,
                pdf_file: None,
                relevance_rank: Some(2),
                relevance_category: Some("second category".into()),
                adoptable_ideas: vec!["second idea".into()],
            },
        ];

        let registry = merge_registry(manifest, Vec::new(), &config);
        assert_eq!(registry.len(), 2);
        assert!(registry
            .iter()
            .any(|record| record.title == "First Web Source"
                && record.relevance_category.as_deref() == Some("first category")));
        assert!(registry
            .iter()
            .any(|record| record.title == "Second Web Source"
                && record.relevance_category.as_deref() == Some("second category")));
    }
}
