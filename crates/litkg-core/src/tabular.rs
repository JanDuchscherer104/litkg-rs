use crate::enrich::infer_enriched_edges;
use crate::model::{
    DownloadMode, NotebookCellKind, ParseStatus, ParsedPaper, ResearchPaper, SourceKind,
};
use crate::notebook::ingest_notebooks_for_research_papers;
use anyhow::{Context, Result};
use arrow_array::{ArrayRef, BooleanArray, Int64Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaperTableRow {
    pub paper_id: String,
    pub citation_key: Option<String>,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<String>,
    pub arxiv_id: Option<String>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub source_kind: SourceKind,
    pub download_mode: DownloadMode,
    pub parse_status: ParseStatus,
    pub has_local_tex: bool,
    pub has_local_pdf: bool,
    pub venue: Option<String>,
    pub task_tags: Vec<String>,
    pub dataset_tags: Vec<String>,
    pub metric_tags: Vec<String>,
    pub code_repositories: Vec<String>,
    pub section_count: usize,
    pub citation_count: usize,
    pub notebook_count: usize,
    pub provenance_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SectionTableRow {
    pub section_id: String,
    pub paper_id: String,
    pub section_index: usize,
    pub level: u8,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CitationTableRow {
    pub citation_row_id: String,
    pub paper_id: String,
    pub citation_index: usize,
    pub citation_text: String,
    pub citation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EdgeTableRow {
    pub edge_id: String,
    pub source_id: String,
    pub source_kind: String,
    pub relation_type: String,
    pub target_id: String,
    pub target_kind: String,
    pub strategy: Option<String>,
    pub score: Option<f64>,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotebookTableRow {
    pub notebook_id: String,
    pub paper_id: String,
    pub path: String,
    pub kernel: Option<String>,
    pub language: Option<String>,
    pub cell_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotebookCellTableRow {
    pub notebook_cell_id: String,
    pub notebook_id: String,
    pub paper_id: String,
    pub cell_index: usize,
    pub cell_kind: NotebookCellKind,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TabularBundle {
    pub papers: Vec<PaperTableRow>,
    pub sections: Vec<SectionTableRow>,
    pub citations: Vec<CitationTableRow>,
    pub edges: Vec<EdgeTableRow>,
    pub notebooks: Vec<NotebookTableRow>,
    pub notebook_cells: Vec<NotebookCellTableRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabularExportPaths {
    pub papers_jsonl: PathBuf,
    pub papers_csv: PathBuf,
    pub papers_parquet: PathBuf,
    pub sections_jsonl: PathBuf,
    pub sections_csv: PathBuf,
    pub citations_jsonl: PathBuf,
    pub citations_csv: PathBuf,
    pub edges_jsonl: PathBuf,
    pub edges_csv: PathBuf,
    pub notebooks_jsonl: PathBuf,
    pub notebooks_csv: PathBuf,
    pub notebook_cells_jsonl: PathBuf,
    pub notebook_cells_csv: PathBuf,
}

pub fn research_papers_from_parsed(parsed_papers: Vec<ParsedPaper>) -> Vec<ResearchPaper> {
    parsed_papers
        .into_iter()
        .map(ResearchPaper::from_parsed)
        .collect()
}

pub fn build_tabular_bundle_from_parsed(parsed_papers: &[ParsedPaper]) -> TabularBundle {
    let research_papers = parsed_papers
        .iter()
        .cloned()
        .map(ResearchPaper::from_parsed)
        .collect::<Vec<_>>();
    build_tabular_bundle(&research_papers)
}

pub fn build_tabular_bundle_from_parsed_with_notebooks(
    parsed_papers: &[ParsedPaper],
    notebook_root: impl AsRef<Path>,
) -> Result<TabularBundle> {
    let mut research_papers = parsed_papers
        .iter()
        .cloned()
        .map(ResearchPaper::from_parsed)
        .collect::<Vec<_>>();
    ingest_notebooks_for_research_papers(&mut research_papers, notebook_root)?;
    Ok(build_tabular_bundle(&research_papers))
}

pub fn build_tabular_bundle(research_papers: &[ResearchPaper]) -> TabularBundle {
    let mut ordered_papers = research_papers.to_vec();
    ordered_papers.sort_by(|left, right| {
        left.parsed
            .metadata
            .paper_id
            .cmp(&right.parsed.metadata.paper_id)
    });

    let mut papers = Vec::new();
    let mut sections = Vec::new();
    let mut citations = Vec::new();
    let mut notebooks = Vec::new();
    let mut notebook_cells = Vec::new();

    for paper in &ordered_papers {
        papers.push(PaperTableRow {
            paper_id: paper.parsed.metadata.paper_id.clone(),
            citation_key: paper.parsed.metadata.citation_key.clone(),
            title: paper.parsed.metadata.title.clone(),
            authors: paper.parsed.metadata.authors.clone(),
            year: paper.parsed.metadata.year.clone(),
            arxiv_id: paper.parsed.metadata.arxiv_id.clone(),
            doi: paper.parsed.metadata.doi.clone(),
            url: paper.parsed.metadata.url.clone(),
            source_kind: paper.parsed.metadata.source_kind.clone(),
            download_mode: paper.parsed.metadata.download_mode.clone(),
            parse_status: paper.parsed.metadata.parse_status.clone(),
            has_local_tex: paper.parsed.metadata.has_local_tex,
            has_local_pdf: paper.parsed.metadata.has_local_pdf,
            venue: paper.research.venue.clone(),
            task_tags: paper.research.task_tags.clone(),
            dataset_tags: paper.research.dataset_tags.clone(),
            metric_tags: paper.research.metric_tags.clone(),
            code_repositories: paper.research.code_repositories.clone(),
            section_count: paper.parsed.sections.len(),
            citation_count: paper.parsed.citations.len(),
            notebook_count: paper.notebooks.len(),
            provenance_count: paper.parsed.provenance.len(),
        });

        for (section_index, section) in paper.parsed.sections.iter().enumerate() {
            sections.push(SectionTableRow {
                section_id: format!("{}:section:{section_index}", paper.parsed.metadata.paper_id),
                paper_id: paper.parsed.metadata.paper_id.clone(),
                section_index,
                level: section.level,
                title: section.title.clone(),
                content: section.content.clone(),
            });
        }

        for (citation_index, citation_text) in paper.parsed.citations.iter().enumerate() {
            let citation_id = normalized_citation_id(citation_text, citation_index);
            citations.push(CitationTableRow {
                citation_row_id: format!(
                    "{}:citation:{citation_index}",
                    paper.parsed.metadata.paper_id
                ),
                paper_id: paper.parsed.metadata.paper_id.clone(),
                citation_index,
                citation_text: citation_text.clone(),
                citation_id,
            });
        }

        for notebook in &paper.notebooks {
            notebooks.push(NotebookTableRow {
                notebook_id: notebook.notebook_id.clone(),
                paper_id: paper.parsed.metadata.paper_id.clone(),
                path: notebook.path.clone(),
                kernel: notebook.kernel.clone(),
                language: notebook.language.clone(),
                cell_count: notebook.cells.len(),
            });
            for cell in &notebook.cells {
                notebook_cells.push(NotebookCellTableRow {
                    notebook_cell_id: format!("{}:cell:{}", notebook.notebook_id, cell.cell_index),
                    notebook_id: notebook.notebook_id.clone(),
                    paper_id: paper.parsed.metadata.paper_id.clone(),
                    cell_index: cell.cell_index,
                    cell_kind: cell.cell_kind.clone(),
                    source: cell.source.clone(),
                });
            }
        }
    }

    sections.sort_by(|left, right| {
        left.paper_id
            .cmp(&right.paper_id)
            .then_with(|| left.section_index.cmp(&right.section_index))
    });
    citations.sort_by(|left, right| {
        left.paper_id
            .cmp(&right.paper_id)
            .then_with(|| left.citation_index.cmp(&right.citation_index))
            .then_with(|| left.citation_id.cmp(&right.citation_id))
    });
    notebooks.sort_by(|left, right| {
        left.paper_id
            .cmp(&right.paper_id)
            .then_with(|| left.notebook_id.cmp(&right.notebook_id))
    });
    notebook_cells.sort_by(|left, right| {
        left.paper_id
            .cmp(&right.paper_id)
            .then_with(|| left.notebook_id.cmp(&right.notebook_id))
            .then_with(|| left.cell_index.cmp(&right.cell_index))
    });

    let parsed_papers = ordered_papers
        .iter()
        .map(|paper| paper.parsed.clone())
        .collect::<Vec<_>>();
    let mut edges = build_edge_rows(&citations, &infer_enriched_edges(&parsed_papers));
    edges.sort_by(|left, right| {
        left.source_id
            .cmp(&right.source_id)
            .then_with(|| left.relation_type.cmp(&right.relation_type))
            .then_with(|| left.target_id.cmp(&right.target_id))
            .then_with(|| left.edge_id.cmp(&right.edge_id))
    });

    TabularBundle {
        papers,
        sections,
        citations,
        edges,
        notebooks,
        notebook_cells,
    }
}

pub fn write_tabular_exports(
    output_root: impl AsRef<Path>,
    bundle: &TabularBundle,
) -> Result<TabularExportPaths> {
    let output_root = output_root.as_ref();
    fs::create_dir_all(output_root)
        .with_context(|| format!("Failed to create {}", output_root.display()))?;

    let paths = TabularExportPaths {
        papers_jsonl: output_root.join("papers.jsonl"),
        papers_csv: output_root.join("papers.csv"),
        papers_parquet: output_root.join("papers.parquet"),
        sections_jsonl: output_root.join("sections.jsonl"),
        sections_csv: output_root.join("sections.csv"),
        citations_jsonl: output_root.join("citations.jsonl"),
        citations_csv: output_root.join("citations.csv"),
        edges_jsonl: output_root.join("edges.jsonl"),
        edges_csv: output_root.join("edges.csv"),
        notebooks_jsonl: output_root.join("notebooks.jsonl"),
        notebooks_csv: output_root.join("notebooks.csv"),
        notebook_cells_jsonl: output_root.join("notebook_cells.jsonl"),
        notebook_cells_csv: output_root.join("notebook_cells.csv"),
    };

    write_jsonl(&paths.papers_jsonl, &bundle.papers)?;
    write_jsonl(&paths.sections_jsonl, &bundle.sections)?;
    write_jsonl(&paths.citations_jsonl, &bundle.citations)?;
    write_jsonl(&paths.edges_jsonl, &bundle.edges)?;
    write_jsonl(&paths.notebooks_jsonl, &bundle.notebooks)?;
    write_jsonl(&paths.notebook_cells_jsonl, &bundle.notebook_cells)?;

    write_papers_csv(&paths.papers_csv, &bundle.papers)?;
    write_papers_parquet(&paths.papers_parquet, &bundle.papers)?;
    write_sections_csv(&paths.sections_csv, &bundle.sections)?;
    write_citations_csv(&paths.citations_csv, &bundle.citations)?;
    write_edges_csv(&paths.edges_csv, &bundle.edges)?;
    write_notebooks_csv(&paths.notebooks_csv, &bundle.notebooks)?;
    write_notebook_cells_csv(&paths.notebook_cells_csv, &bundle.notebook_cells)?;

    Ok(paths)
}

fn write_jsonl<T: Serialize>(path: &Path, rows: &[T]) -> Result<()> {
    let file =
        File::create(path).with_context(|| format!("Failed to create {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    for row in rows {
        serde_json::to_writer(&mut writer, row)
            .with_context(|| format!("Failed to encode JSON row for {}", path.display()))?;
        writer
            .write_all(b"\n")
            .with_context(|| format!("Failed to write newline for {}", path.display()))?;
    }
    writer
        .flush()
        .with_context(|| format!("Failed to flush {}", path.display()))?;
    Ok(())
}

fn build_edge_rows(
    citations: &[CitationTableRow],
    inferred_edges: &[crate::enrich::EnrichedEdge],
) -> Vec<EdgeTableRow> {
    let mut edges = Vec::new();
    let mut seen = BTreeSet::new();

    for citation in citations {
        let edge = EdgeTableRow {
            edge_id: format!(
                "{}:MENTIONS_CITATION:{}",
                citation.paper_id, citation.citation_row_id
            ),
            source_id: citation.paper_id.clone(),
            source_kind: "paper".to_string(),
            relation_type: "MENTIONS_CITATION".to_string(),
            target_id: citation.citation_id.clone(),
            target_kind: "citation".to_string(),
            strategy: None,
            score: None,
            evidence: vec![citation.citation_text.clone()],
        };
        if seen.insert(edge.edge_id.clone()) {
            edges.push(edge);
        }
    }

    for edge in inferred_edges {
        let row = EdgeTableRow {
            edge_id: format!(
                "{}:{}:{}",
                edge.source_paper_id,
                edge.edge_type.rel_type(),
                edge.target_paper_id
            ),
            source_id: edge.source_paper_id.clone(),
            source_kind: "paper".to_string(),
            relation_type: edge.edge_type.rel_type().to_string(),
            target_id: edge.target_paper_id.clone(),
            target_kind: "paper".to_string(),
            strategy: Some(edge.strategy.as_str().to_string()),
            score: Some(edge.score),
            evidence: edge.evidence.clone(),
        };
        if seen.insert(row.edge_id.clone()) {
            edges.push(row);
        }
    }

    edges
}

fn normalized_citation_id(citation: &str, citation_index: usize) -> String {
    let normalized = citation
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if normalized.is_empty() {
        format!("citation_{citation_index}")
    } else {
        normalized
    }
}

fn join_list(items: &[String]) -> String {
    items.join("|")
}

fn write_papers_csv(path: &Path, rows: &[PaperTableRow]) -> Result<()> {
    let mut writer = csv::Writer::from_path(path)
        .with_context(|| format!("Failed to create {}", path.display()))?;
    writer.write_record([
        "paper_id",
        "citation_key",
        "title",
        "authors",
        "year",
        "arxiv_id",
        "doi",
        "url",
        "source_kind",
        "download_mode",
        "parse_status",
        "has_local_tex",
        "has_local_pdf",
        "venue",
        "task_tags",
        "dataset_tags",
        "metric_tags",
        "code_repositories",
        "section_count",
        "citation_count",
        "notebook_count",
        "provenance_count",
    ])?;
    for row in rows {
        writer.write_record([
            row.paper_id.as_str(),
            row.citation_key.as_deref().unwrap_or(""),
            row.title.as_str(),
            &join_list(&row.authors),
            row.year.as_deref().unwrap_or(""),
            row.arxiv_id.as_deref().unwrap_or(""),
            row.doi.as_deref().unwrap_or(""),
            row.url.as_deref().unwrap_or(""),
            &format!("{:?}", row.source_kind),
            &format!("{:?}", row.download_mode),
            &format!("{:?}", row.parse_status),
            &row.has_local_tex.to_string(),
            &row.has_local_pdf.to_string(),
            row.venue.as_deref().unwrap_or(""),
            &join_list(&row.task_tags),
            &join_list(&row.dataset_tags),
            &join_list(&row.metric_tags),
            &join_list(&row.code_repositories),
            &row.section_count.to_string(),
            &row.citation_count.to_string(),
            &row.notebook_count.to_string(),
            &row.provenance_count.to_string(),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

fn write_papers_parquet(path: &Path, rows: &[PaperTableRow]) -> Result<()> {
    let source_kind_values = rows
        .iter()
        .map(|row| format!("{:?}", row.source_kind))
        .collect::<Vec<_>>();
    let download_mode_values = rows
        .iter()
        .map(|row| format!("{:?}", row.download_mode))
        .collect::<Vec<_>>();
    let parse_status_values = rows
        .iter()
        .map(|row| format!("{:?}", row.parse_status))
        .collect::<Vec<_>>();

    let schema = Arc::new(Schema::new(vec![
        Field::new("paper_id", DataType::Utf8, false),
        Field::new("citation_key", DataType::Utf8, true),
        Field::new("title", DataType::Utf8, false),
        Field::new("authors", DataType::Utf8, false),
        Field::new("year", DataType::Utf8, true),
        Field::new("arxiv_id", DataType::Utf8, true),
        Field::new("doi", DataType::Utf8, true),
        Field::new("url", DataType::Utf8, true),
        Field::new("source_kind", DataType::Utf8, false),
        Field::new("download_mode", DataType::Utf8, false),
        Field::new("parse_status", DataType::Utf8, false),
        Field::new("has_local_tex", DataType::Boolean, false),
        Field::new("has_local_pdf", DataType::Boolean, false),
        Field::new("venue", DataType::Utf8, true),
        Field::new("task_tags", DataType::Utf8, false),
        Field::new("dataset_tags", DataType::Utf8, false),
        Field::new("metric_tags", DataType::Utf8, false),
        Field::new("code_repositories", DataType::Utf8, false),
        Field::new("section_count", DataType::Int64, false),
        Field::new("citation_count", DataType::Int64, false),
        Field::new("notebook_count", DataType::Int64, false),
        Field::new("provenance_count", DataType::Int64, false),
    ]));

    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(
            rows.iter()
                .map(|row| Some(row.paper_id.as_str()))
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter()
                .map(|row| row.citation_key.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter()
                .map(|row| Some(row.title.as_str()))
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter()
                .map(|row| Some(join_list(&row.authors)))
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter()
                .map(|row| row.year.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter()
                .map(|row| row.arxiv_id.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter()
                .map(|row| row.doi.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter()
                .map(|row| row.url.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            source_kind_values
                .iter()
                .map(|value| Some(value.as_str()))
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            download_mode_values
                .iter()
                .map(|value| Some(value.as_str()))
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            parse_status_values
                .iter()
                .map(|value| Some(value.as_str()))
                .collect::<Vec<_>>(),
        )),
        Arc::new(BooleanArray::from(
            rows.iter().map(|row| row.has_local_tex).collect::<Vec<_>>(),
        )),
        Arc::new(BooleanArray::from(
            rows.iter().map(|row| row.has_local_pdf).collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter()
                .map(|row| row.venue.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter()
                .map(|row| Some(join_list(&row.task_tags)))
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter()
                .map(|row| Some(join_list(&row.dataset_tags)))
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter()
                .map(|row| Some(join_list(&row.metric_tags)))
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter()
                .map(|row| Some(join_list(&row.code_repositories)))
                .collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            rows.iter()
                .map(|row| row.section_count as i64)
                .collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            rows.iter()
                .map(|row| row.citation_count as i64)
                .collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            rows.iter()
                .map(|row| row.notebook_count as i64)
                .collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            rows.iter()
                .map(|row| row.provenance_count as i64)
                .collect::<Vec<_>>(),
        )),
    ];

    let batch = RecordBatch::try_new(schema.clone(), columns)
        .with_context(|| format!("Failed to build papers batch for {}", path.display()))?;

    let file =
        File::create(path).with_context(|| format!("Failed to create {}", path.display()))?;
    let props = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .build();
    let mut writer = ArrowWriter::try_new(file, schema, Some(props))
        .with_context(|| format!("Failed to start parquet writer for {}", path.display()))?;
    writer
        .write(&batch)
        .with_context(|| format!("Failed to write parquet batch for {}", path.display()))?;
    writer
        .close()
        .with_context(|| format!("Failed to finalize parquet file {}", path.display()))?;
    Ok(())
}

fn write_sections_csv(path: &Path, rows: &[SectionTableRow]) -> Result<()> {
    let mut writer = csv::Writer::from_path(path)
        .with_context(|| format!("Failed to create {}", path.display()))?;
    writer.write_record([
        "section_id",
        "paper_id",
        "section_index",
        "level",
        "title",
        "content",
    ])?;
    for row in rows {
        writer.write_record([
            row.section_id.as_str(),
            row.paper_id.as_str(),
            &row.section_index.to_string(),
            &row.level.to_string(),
            row.title.as_str(),
            row.content.as_str(),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

fn write_citations_csv(path: &Path, rows: &[CitationTableRow]) -> Result<()> {
    let mut writer = csv::Writer::from_path(path)
        .with_context(|| format!("Failed to create {}", path.display()))?;
    writer.write_record([
        "citation_row_id",
        "paper_id",
        "citation_index",
        "citation_text",
        "citation_id",
    ])?;
    for row in rows {
        writer.write_record([
            row.citation_row_id.as_str(),
            row.paper_id.as_str(),
            &row.citation_index.to_string(),
            row.citation_text.as_str(),
            row.citation_id.as_str(),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

fn write_edges_csv(path: &Path, rows: &[EdgeTableRow]) -> Result<()> {
    let mut writer = csv::Writer::from_path(path)
        .with_context(|| format!("Failed to create {}", path.display()))?;
    writer.write_record([
        "edge_id",
        "source_id",
        "source_kind",
        "relation_type",
        "target_id",
        "target_kind",
        "strategy",
        "score",
        "evidence",
    ])?;
    for row in rows {
        writer.write_record([
            row.edge_id.as_str(),
            row.source_id.as_str(),
            row.source_kind.as_str(),
            row.relation_type.as_str(),
            row.target_id.as_str(),
            row.target_kind.as_str(),
            row.strategy.as_deref().unwrap_or(""),
            &row.score.map(|value| value.to_string()).unwrap_or_default(),
            &join_list(&row.evidence),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

fn write_notebooks_csv(path: &Path, rows: &[NotebookTableRow]) -> Result<()> {
    let mut writer = csv::Writer::from_path(path)
        .with_context(|| format!("Failed to create {}", path.display()))?;
    writer.write_record([
        "notebook_id",
        "paper_id",
        "path",
        "kernel",
        "language",
        "cell_count",
    ])?;
    for row in rows {
        writer.write_record([
            row.notebook_id.as_str(),
            row.paper_id.as_str(),
            row.path.as_str(),
            row.kernel.as_deref().unwrap_or(""),
            row.language.as_deref().unwrap_or(""),
            &row.cell_count.to_string(),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

fn write_notebook_cells_csv(path: &Path, rows: &[NotebookCellTableRow]) -> Result<()> {
    let mut writer = csv::Writer::from_path(path)
        .with_context(|| format!("Failed to create {}", path.display()))?;
    writer.write_record([
        "notebook_cell_id",
        "notebook_id",
        "paper_id",
        "cell_index",
        "cell_kind",
        "source",
    ])?;
    for row in rows {
        writer.write_record([
            row.notebook_cell_id.as_str(),
            row.notebook_id.as_str(),
            row.paper_id.as_str(),
            &row.cell_index.to_string(),
            &format!("{:?}", row.cell_kind),
            row.source.as_str(),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        DownloadMode, NotebookCell, NotebookDocument, PaperFigure, PaperSection, PaperSourceRecord,
        PaperTable, ParseStatus, ResearchMetadata, SourceKind,
    };
    use parquet::file::reader::{FileReader, SerializedFileReader};

    fn parsed_paper(
        paper_id: &str,
        citation_key: Option<&str>,
        citations: Vec<&str>,
    ) -> ParsedPaper {
        ParsedPaper {
            metadata: PaperSourceRecord {
                paper_id: paper_id.to_string(),
                citation_key: citation_key.map(str::to_string),
                title: format!("Title {paper_id}"),
                authors: vec!["Alice".to_string(), "Bob".to_string()],
                year: Some("2026".to_string()),
                arxiv_id: Some("2601.00001".to_string()),
                doi: None,
                url: Some("https://example.org/paper".to_string()),
                tex_dir: None,
                pdf_file: None,
                source_kind: SourceKind::Bib,
                download_mode: DownloadMode::MetadataOnly,
                has_local_tex: false,
                has_local_pdf: false,
                parse_status: ParseStatus::MetadataOnly,
                semantic_scholar: None,
            },
            abstract_text: Some("A practical abstract.".to_string()),
            sections: vec![PaperSection {
                level: 1,
                title: "Intro".to_string(),
                content: "Section content".to_string(),
            }],
            figures: vec![PaperFigure {
                caption: "Figure caption".to_string(),
            }],
            tables: vec![PaperTable {
                caption: "Table caption".to_string(),
            }],
            citations: citations.into_iter().map(str::to_string).collect(),
            provenance: vec!["registry".to_string()],
        }
    }

    #[test]
    fn builds_tabular_bundle_with_research_metadata_and_notebook_rows() {
        let mut first = ResearchPaper::from_parsed(parsed_paper(
            "paper-a",
            Some("paperA2026"),
            vec!["paperB2026", "Unknown-Key"],
        ));
        first.research = ResearchMetadata {
            venue: Some("ICML".to_string()),
            task_tags: vec!["slam".to_string()],
            dataset_tags: vec!["advio".to_string()],
            metric_tags: vec!["ate".to_string()],
            code_repositories: vec!["https://github.com/example/repo".to_string()],
        };
        first.notebooks = vec![NotebookDocument {
            notebook_id: "nb-1".to_string(),
            path: "notebooks/analysis.ipynb".to_string(),
            kernel: Some("python3".to_string()),
            language: Some("python".to_string()),
            cells: vec![
                NotebookCell {
                    cell_index: 0,
                    cell_kind: NotebookCellKind::Markdown,
                    source: "# Intro".to_string(),
                },
                NotebookCell {
                    cell_index: 1,
                    cell_kind: NotebookCellKind::Code,
                    source: "print('hi')".to_string(),
                },
            ],
        }];

        let second =
            ResearchPaper::from_parsed(parsed_paper("paper-b", Some("paperB2026"), vec![]));

        let bundle = build_tabular_bundle(&[first, second]);

        assert_eq!(bundle.papers.len(), 2);
        assert_eq!(bundle.sections.len(), 2);
        assert_eq!(bundle.citations.len(), 2);
        assert_eq!(bundle.notebooks.len(), 1);
        assert_eq!(bundle.notebook_cells.len(), 2);

        let paper_row = bundle
            .papers
            .iter()
            .find(|row| row.paper_id == "paper-a")
            .expect("paper row exists");
        assert_eq!(paper_row.venue.as_deref(), Some("ICML"));
        assert_eq!(paper_row.task_tags, vec!["slam".to_string()]);

        assert!(bundle
            .edges
            .iter()
            .any(|row| row.relation_type == "MENTIONS_CITATION"));
        assert!(bundle
            .edges
            .iter()
            .any(|row| row.relation_type == "CITES_PAPER"));
    }

    #[test]
    fn writes_tabular_exports_to_jsonl_and_csv() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let bundle = build_tabular_bundle_from_parsed(&[parsed_paper(
            "paper-a",
            Some("paperA2026"),
            vec!["paperA2026"],
        )]);

        let paths = write_tabular_exports(dir.path(), &bundle).expect("writes exports");

        assert!(paths.papers_jsonl.exists());
        assert!(paths.papers_csv.exists());
        assert!(paths.papers_parquet.exists());
        assert!(paths.sections_jsonl.exists());
        assert!(paths.sections_csv.exists());
        assert!(paths.citations_jsonl.exists());
        assert!(paths.citations_csv.exists());
        assert!(paths.edges_jsonl.exists());
        assert!(paths.edges_csv.exists());
        assert!(paths.notebooks_jsonl.exists());
        assert!(paths.notebooks_csv.exists());
        assert!(paths.notebook_cells_jsonl.exists());
        assert!(paths.notebook_cells_csv.exists());

        let papers_jsonl = std::fs::read_to_string(paths.papers_jsonl).expect("read papers jsonl");
        assert!(papers_jsonl.contains("\"paper_id\":\"paper-a\""));

        let edges_csv = std::fs::read_to_string(paths.edges_csv).expect("read edges csv");
        assert!(edges_csv.contains("relation_type"));
        assert!(edges_csv.contains("MENTIONS_CITATION"));

        let parquet_reader = SerializedFileReader::new(
            std::fs::File::open(paths.papers_parquet).expect("open parquet"),
        )
        .expect("read parquet");
        assert_eq!(parquet_reader.metadata().file_metadata().num_rows(), 1);
    }

    #[test]
    fn builds_tabular_bundle_with_notebooks_from_ipynb_files() {
        let dir = tempfile::tempdir().expect("create temp dir");
        std::fs::write(
            dir.path().join("paper-a_notes.ipynb"),
            r##"{
              "metadata": {
                "kernelspec": {"name": "python3", "language": "python"}
              },
              "cells": [
                {"cell_type": "markdown", "source": ["# Notes\n"]},
                {"cell_type": "code", "source": ["x = 1\n", "x"]}
              ]
            }"##,
        )
        .expect("write notebook");

        let bundle = build_tabular_bundle_from_parsed_with_notebooks(
            &[parsed_paper("paper-a", Some("paperA2026"), vec![])],
            dir.path(),
        )
        .expect("build bundle");

        assert_eq!(bundle.notebooks.len(), 1);
        assert_eq!(bundle.notebooks[0].paper_id, "paper-a");
        assert_eq!(bundle.notebook_cells.len(), 2);
        assert_eq!(bundle.notebook_cells[1].cell_kind, NotebookCellKind::Code);
    }
}
