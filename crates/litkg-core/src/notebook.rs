use crate::model::{NotebookCell, NotebookCellKind, NotebookDocument, ResearchPaper};
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotebookIngestStats {
    pub scanned_files: usize,
    pub attached_notebooks: usize,
    pub unmatched_notebooks: usize,
}

pub fn load_notebook_documents(notebook_root: impl AsRef<Path>) -> Result<Vec<NotebookDocument>> {
    let notebook_root = notebook_root.as_ref();
    if !notebook_root.exists() {
        return Ok(Vec::new());
    }

    let mut documents = Vec::new();
    let mut notebook_paths = Vec::new();
    for entry in WalkDir::new(notebook_root) {
        let entry = entry
            .with_context(|| format!("Failed to walk notebook root {}", notebook_root.display()))?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("ipynb") {
            notebook_paths.push(path.to_path_buf());
        }
    }
    notebook_paths.sort();

    for path in notebook_paths {
        documents.push(parse_notebook_document(notebook_root, &path)?);
    }
    Ok(documents)
}

pub fn ingest_notebooks_for_research_papers(
    research_papers: &mut [ResearchPaper],
    notebook_root: impl AsRef<Path>,
) -> Result<NotebookIngestStats> {
    let notebook_documents = load_notebook_documents(notebook_root)?;

    let mut paper_index_by_id = BTreeMap::new();
    for (index, paper) in research_papers.iter().enumerate() {
        paper_index_by_id.insert(paper.parsed.metadata.paper_id.clone(), index);
    }

    let mut attached = 0usize;
    let mut unmatched = 0usize;
    for document in notebook_documents {
        let Some(paper_id) = resolve_notebook_paper_id(&document, &paper_index_by_id) else {
            unmatched += 1;
            continue;
        };
        if let Some(index) = paper_index_by_id.get(&paper_id).copied() {
            research_papers[index].notebooks.push(document);
            attached += 1;
        } else {
            unmatched += 1;
        }
    }

    for paper in research_papers {
        paper.notebooks.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then_with(|| left.notebook_id.cmp(&right.notebook_id))
        });
    }

    Ok(NotebookIngestStats {
        scanned_files: attached + unmatched,
        attached_notebooks: attached,
        unmatched_notebooks: unmatched,
    })
}

fn parse_notebook_document(notebook_root: &Path, notebook_path: &Path) -> Result<NotebookDocument> {
    let raw = fs::read_to_string(notebook_path)
        .with_context(|| format!("Failed to read notebook {}", notebook_path.display()))?;
    let notebook_json: Value = serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse notebook {}", notebook_path.display()))?;

    let relative_path = notebook_path
        .strip_prefix(notebook_root)
        .unwrap_or(notebook_path)
        .to_string_lossy()
        .replace('\\', "/");
    let default_notebook_id = relative_path.trim_end_matches(".ipynb").to_string();

    let notebook_id = notebook_json
        .get("metadata")
        .and_then(|metadata| metadata.get("litkg"))
        .and_then(|litkg| litkg.get("notebook_id"))
        .and_then(Value::as_str)
        .unwrap_or(default_notebook_id.as_str())
        .to_string();

    let kernel = notebook_json
        .get("metadata")
        .and_then(|metadata| metadata.get("kernelspec"))
        .and_then(|kernel_spec| kernel_spec.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let language = notebook_json
        .get("metadata")
        .and_then(|metadata| metadata.get("language_info"))
        .and_then(|language_info| language_info.get("name"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            notebook_json
                .get("metadata")
                .and_then(|metadata| metadata.get("kernelspec"))
                .and_then(|kernel_spec| kernel_spec.get("language"))
                .and_then(Value::as_str)
                .map(str::to_string)
        });

    let cells = notebook_json
        .get("cells")
        .and_then(Value::as_array)
        .map(|raw_cells| {
            raw_cells
                .iter()
                .enumerate()
                .map(|(cell_index, cell)| NotebookCell {
                    cell_index,
                    cell_kind: parse_cell_kind(cell),
                    source: parse_cell_source(cell),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(NotebookDocument {
        notebook_id,
        path: relative_path,
        kernel,
        language,
        cells,
    })
}

fn resolve_notebook_paper_id(
    notebook: &NotebookDocument,
    paper_index_by_id: &BTreeMap<String, usize>,
) -> Option<String> {
    if let Some(paper_id) =
        parse_paper_id_from_notebook_id(&notebook.notebook_id, paper_index_by_id)
    {
        return Some(paper_id);
    }
    parse_paper_id_from_path(&notebook.path, paper_index_by_id)
}

fn parse_paper_id_from_notebook_id(
    notebook_id: &str,
    paper_index_by_id: &BTreeMap<String, usize>,
) -> Option<String> {
    let lower = notebook_id.to_ascii_lowercase();
    let mut exact = None;
    let mut prefix_match = None;
    for paper_id in paper_index_by_id.keys() {
        let paper_id_lower = paper_id.to_ascii_lowercase();
        if lower == paper_id_lower {
            exact = Some(paper_id.clone());
            break;
        }
        if lower.starts_with(&format!("{paper_id_lower}_"))
            || lower.starts_with(&format!("{paper_id_lower}-"))
            || lower.starts_with(&format!("{paper_id_lower}/"))
        {
            prefix_match = Some(paper_id.clone());
        }
    }
    exact.or(prefix_match)
}

fn parse_paper_id_from_path(
    notebook_path: &str,
    paper_index_by_id: &BTreeMap<String, usize>,
) -> Option<String> {
    let normalized_path = notebook_path.to_ascii_lowercase();
    for paper_id in paper_index_by_id.keys() {
        let paper_id_lower = paper_id.to_ascii_lowercase();
        if normalized_path.starts_with(&(paper_id_lower.clone() + "/"))
            || normalized_path
                .split('/')
                .any(|segment| segment == paper_id_lower.as_str())
        {
            return Some(paper_id.clone());
        }
    }

    let file_name = notebook_path.rsplit('/').next().unwrap_or(notebook_path);
    let stem = file_name.strip_suffix(".ipynb").unwrap_or(file_name);
    parse_paper_id_from_notebook_id(stem, paper_index_by_id)
}

fn parse_cell_kind(cell: &Value) -> NotebookCellKind {
    match cell
        .get("cell_type")
        .and_then(Value::as_str)
        .unwrap_or("raw")
        .to_ascii_lowercase()
        .as_str()
    {
        "code" => NotebookCellKind::Code,
        "markdown" => NotebookCellKind::Markdown,
        _ => NotebookCellKind::Raw,
    }
}

fn parse_cell_source(cell: &Value) -> String {
    match cell.get("source") {
        Some(Value::String(source)) => source.clone(),
        Some(Value::Array(lines)) => lines
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        DownloadMode, PaperFigure, PaperSection, PaperSourceRecord, PaperTable, ParseStatus,
        ParsedPaper, ResearchPaper, SourceKind,
    };

    fn sample_research_paper(paper_id: &str) -> ResearchPaper {
        ResearchPaper::from_parsed(ParsedPaper {
            kind: crate::model::DocumentKind::Literature,
            metadata: PaperSourceRecord {
                paper_id: paper_id.to_string(),
                citation_key: Some(format!("{paper_id}_key")),
                title: format!("Title {paper_id}"),
                authors: vec!["Alice".to_string()],
                year: Some("2026".to_string()),
                arxiv_id: None,
                doi: None,
                url: None,
                tex_dir: None,
                pdf_file: None,
                source_kind: SourceKind::Bib,
                download_mode: DownloadMode::MetadataOnly,
                has_local_tex: false,
                has_local_pdf: false,
                parse_status: ParseStatus::MetadataOnly,
                semantic_scholar: None,
            },
            abstract_text: None,
            sections: vec![PaperSection {
                level: 1,
                title: "Intro".to_string(),
                content: "Section".to_string(),
            }],
            figures: vec![PaperFigure {
                caption: "Figure".to_string(),
            }],
            tables: vec![PaperTable {
                caption: "Table".to_string(),
            }],
            citations: vec![],
            citation_references: Vec::new(),
            provenance: vec!["registry".to_string()],
        })
    }

    #[test]
    fn loads_notebook_documents_from_ipynb() {
        let dir = tempfile::tempdir().expect("temp dir");
        let notebook_path = dir.path().join("paper-a_notes.ipynb");
        fs::write(
            &notebook_path,
            r##"{
              "metadata": {
                "kernelspec": {"name": "python3", "language": "python"},
                "language_info": {"name": "python"},
                "litkg": {"notebook_id": "paper-a-notebook"}
              },
              "cells": [
                {"cell_type": "markdown", "source": ["# Intro\n"]},
                {"cell_type": "code", "source": ["print('hello')\n"]}
              ]
            }"##,
        )
        .expect("write notebook");

        let notebooks = load_notebook_documents(dir.path()).expect("load notebooks");
        assert_eq!(notebooks.len(), 1);
        assert_eq!(notebooks[0].notebook_id, "paper-a-notebook");
        assert_eq!(notebooks[0].kernel.as_deref(), Some("python3"));
        assert_eq!(notebooks[0].cells.len(), 2);
        assert_eq!(notebooks[0].cells[0].cell_kind, NotebookCellKind::Markdown);
        assert_eq!(notebooks[0].cells[1].cell_kind, NotebookCellKind::Code);
    }

    #[test]
    fn ingests_notebooks_into_research_papers() {
        let dir = tempfile::tempdir().expect("temp dir");
        let nested_root = dir.path().join("paper-a").join("experiments");
        fs::create_dir_all(&nested_root).expect("create nested notebook directory");
        fs::write(
            nested_root.join("analysis.ipynb"),
            r##"{
              "metadata": {"kernelspec": {"name": "python3"}},
              "cells": [{"cell_type": "markdown", "source": "A"}]
            }"##,
        )
        .expect("write notebook a");
        fs::write(
            dir.path().join("unknown.ipynb"),
            r#"{"metadata": {}, "cells": []}"#,
        )
        .expect("write notebook unknown");

        let mut papers = vec![
            sample_research_paper("paper-a"),
            sample_research_paper("paper-b"),
        ];
        let stats = ingest_notebooks_for_research_papers(&mut papers, dir.path())
            .expect("ingest notebooks");

        assert_eq!(stats.scanned_files, 2);
        assert_eq!(stats.attached_notebooks, 1);
        assert_eq!(stats.unmatched_notebooks, 1);
        assert_eq!(papers[0].notebooks.len(), 1);
        assert_eq!(papers[1].notebooks.len(), 0);
    }
}
