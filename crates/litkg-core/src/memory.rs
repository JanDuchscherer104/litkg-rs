use crate::{ParsedPaper, RepoConfig};
use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

const PROJECT_STATE_FILE: &str = "PROJECT_STATE.md";
const DECISIONS_FILE: &str = "DECISIONS.md";
const OPEN_QUESTIONS_FILE: &str = "OPEN_QUESTIONS.md";
const GOTCHAS_FILE: &str = "GOTCHAS.md";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MemoryNodeKind {
    ProjectState,
    Decision,
    OpenQuestion,
    Gotcha,
}

impl MemoryNodeKind {
    fn uses_bullet_chunks(self) -> bool {
        matches!(
            self,
            MemoryNodeKind::Decision | MemoryNodeKind::OpenQuestion | MemoryNodeKind::Gotcha
        )
    }

    fn id_prefix(self) -> &'static str {
        match self {
            MemoryNodeKind::ProjectState => "project_state",
            MemoryNodeKind::Decision => "decision",
            MemoryNodeKind::OpenQuestion => "open_question",
            MemoryNodeKind::Gotcha => "gotcha",
        }
    }

    fn is_constraint_source(self) -> bool {
        matches!(self, MemoryNodeKind::Decision | MemoryNodeKind::Gotcha)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MemoryChunkKind {
    Section,
    Bullet,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRelationType {
    DocumentsCode,
    Constrains,
    RelatesTo,
    Supersedes,
}

impl MemoryRelationType {
    pub fn rel_type(self) -> &'static str {
        match self {
            MemoryRelationType::DocumentsCode => "DOCUMENTS_CODE",
            MemoryRelationType::Constrains => "CONSTRAINS",
            MemoryRelationType::RelatesTo => "RELATES_TO",
            MemoryRelationType::Supersedes => "SUPERSEDES",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MemorySurfaceKind {
    Code,
    Doc,
    Paper,
}

impl MemorySurfaceKind {
    fn kind_name(self) -> &'static str {
        match self {
            MemorySurfaceKind::Code => "code_surface",
            MemorySurfaceKind::Doc => "doc_surface",
            MemorySurfaceKind::Paper => "paper_surface",
        }
    }

    fn id_prefix(self) -> &'static str {
        match self {
            MemorySurfaceKind::Code => "code_surface",
            MemorySurfaceKind::Doc => "doc_surface",
            MemorySurfaceKind::Paper => "paper_surface",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryNode {
    pub id: String,
    pub kind: MemoryNodeKind,
    pub chunk_kind: MemoryChunkKind,
    pub title: String,
    pub text: String,
    pub source_path: String,
    pub document_id: String,
    pub document_title: String,
    pub section_heading: String,
    pub section_slug: String,
    pub chunk_ordinal: usize,
    pub line_start: usize,
    pub line_end: usize,
    pub snapshot_kind: String,
    pub snapshot_value: String,
    pub source_updated: Option<String>,
    pub source_scope: Option<String>,
    pub source_owner: Option<String>,
    pub source_status: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemorySurface {
    pub id: String,
    pub kind: MemorySurfaceKind,
    pub locator: String,
    pub repo_path: Option<String>,
    pub symbol: Option<String>,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryRelation {
    pub source_id: String,
    pub target_id: String,
    pub relation_type: MemoryRelationType,
    pub target_kind: String,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryImportBundle {
    #[serde(default)]
    pub nodes: Vec<MemoryNode>,
    #[serde(default)]
    pub surfaces: Vec<MemorySurface>,
    #[serde(default)]
    pub relations: Vec<MemoryRelation>,
}

impl MemoryImportBundle {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.surfaces.is_empty() && self.relations.is_empty()
    }
}

#[derive(Debug, Default)]
struct Frontmatter {
    id: Option<String>,
    updated: Option<String>,
    scope: Option<String>,
    owner: Option<String>,
    status: Option<String>,
    tags: Vec<String>,
}

#[derive(Debug)]
struct MemoryDocumentSpec {
    file_name: &'static str,
    kind: MemoryNodeKind,
}

#[derive(Debug)]
struct ParsedDocument {
    nodes: Vec<MemoryNode>,
    explicit_supersedes: Vec<(String, Vec<String>)>,
}

#[derive(Debug)]
struct ParsedSection {
    heading: String,
    slug: String,
    lines: Vec<LineEntry>,
}

#[derive(Debug, Clone)]
struct LineEntry {
    line_number: usize,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ReferenceCandidate {
    Path(String),
    Symbol(String),
    CitationKey(String),
    ArxivId(String),
}

#[derive(Debug, Clone)]
enum ResolvedTarget {
    ExistingPaper { id: String, target_kind: String },
    Surface(MemorySurface),
}

#[derive(Debug, Default)]
struct PaperLookup {
    by_paper_id: BTreeMap<String, String>,
    by_citation_key: BTreeMap<String, String>,
    by_arxiv_id: BTreeMap<String, String>,
}

impl PaperLookup {
    fn from_papers(papers: &[ParsedPaper]) -> Self {
        let mut lookup = Self::default();
        for paper in papers {
            let paper_id = paper.metadata.paper_id.clone();
            lookup.by_paper_id.insert(
                paper.metadata.paper_id.to_ascii_lowercase(),
                paper_id.clone(),
            );
            if let Some(citation_key) = &paper.metadata.citation_key {
                lookup
                    .by_citation_key
                    .insert(citation_key.to_ascii_lowercase(), paper_id.clone());
            }
            if let Some(arxiv_id) = &paper.metadata.arxiv_id {
                lookup
                    .by_arxiv_id
                    .insert(arxiv_id.to_ascii_lowercase(), paper_id.clone());
            }
        }
        lookup
    }

    fn resolve_citation_key(&self, citation_key: &str) -> Option<String> {
        self.by_citation_key
            .get(&citation_key.to_ascii_lowercase())
            .cloned()
    }

    fn resolve_arxiv_id(&self, arxiv_id: &str) -> Option<String> {
        self.by_arxiv_id
            .get(&arxiv_id.to_ascii_lowercase())
            .cloned()
    }

    fn resolve_path_stem(&self, path: &str) -> Option<String> {
        let stem = Path::new(path).file_stem()?.to_str()?;
        self.by_paper_id.get(&stem.to_ascii_lowercase()).cloned()
    }
}

pub fn load_project_memory(
    config: &RepoConfig,
    papers: &[ParsedPaper],
) -> Result<MemoryImportBundle> {
    let Some(memory_state_root) = config.memory_state_root() else {
        return Ok(MemoryImportBundle::default());
    };
    if !memory_state_root.exists() {
        anyhow::bail!(
            "Configured memory_state_root {} does not exist",
            memory_state_root.display()
        );
    }

    let repo_root = infer_repo_root_from_memory_root(&memory_state_root);
    let paper_lookup = PaperLookup::from_papers(papers);
    let mut nodes = Vec::new();
    let mut surfaces = BTreeMap::new();
    let mut relation_map = BTreeMap::new();

    for spec in memory_document_specs() {
        let path = memory_state_root.join(spec.file_name);
        if !path.is_file() {
            continue;
        }
        let parsed = parse_memory_document(&path, repo_root.as_deref(), spec.kind)?;
        for node in parsed.nodes {
            for reference in extract_reference_candidates(&node.text) {
                if let Some(target) =
                    resolve_reference_candidate(&reference, repo_root.as_deref(), &paper_lookup)
                {
                    let (target_id, target_kind) = match target {
                        ResolvedTarget::ExistingPaper { id, target_kind } => (id, target_kind),
                        ResolvedTarget::Surface(surface) => {
                            let target_id = surface.id.clone();
                            let target_kind = surface.kind.kind_name().to_string();
                            surfaces.entry(surface.id.clone()).or_insert(surface);
                            (target_id, target_kind)
                        }
                    };
                    for relation_type in relation_types_for(node.kind, &target_kind) {
                        add_relation(
                            &mut relation_map,
                            &node.id,
                            &target_id,
                            relation_type,
                            &target_kind,
                            reference.raw_value(),
                        );
                    }
                }
            }

            nodes.push(node);
        }

        for (source_id, superseded_ids) in parsed.explicit_supersedes {
            for target_id in superseded_ids {
                add_relation(
                    &mut relation_map,
                    &source_id,
                    &target_id,
                    MemoryRelationType::Supersedes,
                    "memory_node",
                    "explicit supersedes directive".to_string(),
                );
            }
        }
    }

    nodes.sort_by(|left, right| left.id.cmp(&right.id));
    let mut surfaces = surfaces.into_values().collect::<Vec<_>>();
    surfaces.sort_by(|left, right| left.id.cmp(&right.id));
    let relations = relation_map
        .into_iter()
        .map(
            |((source_id, target_id, relation_type, target_kind), evidence)| MemoryRelation {
                source_id,
                target_id,
                relation_type,
                target_kind,
                evidence: evidence.into_iter().collect(),
            },
        )
        .collect::<Vec<_>>();

    Ok(MemoryImportBundle {
        nodes,
        surfaces,
        relations,
    })
}

fn memory_document_specs() -> &'static [MemoryDocumentSpec] {
    &[
        MemoryDocumentSpec {
            file_name: PROJECT_STATE_FILE,
            kind: MemoryNodeKind::ProjectState,
        },
        MemoryDocumentSpec {
            file_name: DECISIONS_FILE,
            kind: MemoryNodeKind::Decision,
        },
        MemoryDocumentSpec {
            file_name: OPEN_QUESTIONS_FILE,
            kind: MemoryNodeKind::OpenQuestion,
        },
        MemoryDocumentSpec {
            file_name: GOTCHAS_FILE,
            kind: MemoryNodeKind::Gotcha,
        },
    ]
}

fn parse_memory_document(
    path: &Path,
    repo_root: Option<&Path>,
    kind: MemoryNodeKind,
) -> Result<ParsedDocument> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let lines = raw.lines().collect::<Vec<_>>();
    let (frontmatter, body_start) = parse_frontmatter(&lines);
    let document_title = extract_document_title(&lines[body_start..]).unwrap_or_else(|| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("memory")
            .replace('_', " ")
    });
    let source_path = repo_relative_path(path, repo_root);
    let snapshot_value = snapshot_value(path)?;
    let sections = parse_sections(&lines, body_start, &document_title);
    let mut nodes = Vec::new();
    let mut explicit_supersedes = Vec::new();
    let document_id = frontmatter.id.clone().unwrap_or_else(|| {
        slugify(
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("memory"),
        )
    });

    for section in sections {
        if kind.uses_bullet_chunks() {
            let bullet_chunks = parse_bullet_chunks(&section);
            if bullet_chunks.is_empty() {
                if let Some(chunk) = build_section_chunk(
                    kind,
                    &source_path,
                    &document_id,
                    &document_title,
                    &section,
                    &snapshot_value,
                    &frontmatter,
                )? {
                    explicit_supersedes.push((chunk.id.clone(), chunk.supersedes));
                    nodes.push(chunk.node);
                }
                continue;
            }

            for (chunk_ordinal, bullet_chunk) in bullet_chunks.into_iter().enumerate() {
                let raw_text = join_chunk_lines(&bullet_chunk.lines, MemoryChunkKind::Bullet);
                let (text, supersedes) = extract_supersedes_directives(&raw_text);
                if text.is_empty() {
                    continue;
                }
                let title = text.lines().next().unwrap_or("").trim().to_string();
                let line_start = bullet_chunk
                    .lines
                    .first()
                    .map(|line| line.line_number)
                    .unwrap_or(0);
                let line_end = bullet_chunk
                    .lines
                    .last()
                    .map(|line| line.line_number)
                    .unwrap_or(line_start);
                let node = build_memory_node(
                    kind,
                    MemoryChunkKind::Bullet,
                    &source_path,
                    &document_id,
                    &document_title,
                    &section.heading,
                    &section.slug,
                    chunk_ordinal + 1,
                    &title,
                    &text,
                    line_start,
                    line_end,
                    &snapshot_value,
                    &frontmatter,
                );
                explicit_supersedes.push((node.id.clone(), supersedes));
                nodes.push(node);
            }
        } else if let Some(chunk) = build_section_chunk(
            kind,
            &source_path,
            &document_id,
            &document_title,
            &section,
            &snapshot_value,
            &frontmatter,
        )? {
            explicit_supersedes.push((chunk.id.clone(), chunk.supersedes));
            nodes.push(chunk.node);
        }
    }

    Ok(ParsedDocument {
        nodes,
        explicit_supersedes,
    })
}

fn parse_frontmatter(lines: &[&str]) -> (Frontmatter, usize) {
    if lines.first().copied() != Some("---") {
        return (Frontmatter::default(), 0);
    }

    let mut frontmatter = Frontmatter::default();
    for (index, line) in lines.iter().enumerate().skip(1) {
        if *line == "---" {
            return (frontmatter, index + 1);
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "id" => frontmatter.id = Some(value.to_string()),
                "updated" => frontmatter.updated = Some(value.to_string()),
                "scope" => frontmatter.scope = Some(value.to_string()),
                "owner" => frontmatter.owner = Some(value.to_string()),
                "status" => frontmatter.status = Some(value.to_string()),
                "tags" => frontmatter.tags = parse_inline_list(value),
                _ => {}
            }
        }
    }

    (frontmatter, 0)
}

fn extract_document_title(body_lines: &[&str]) -> Option<String> {
    body_lines.iter().find_map(|line| {
        line.strip_prefix("# ")
            .map(|title| title.trim().to_string())
    })
}

fn parse_sections(lines: &[&str], body_start: usize, document_title: &str) -> Vec<ParsedSection> {
    let mut sections = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_lines = Vec::new();

    for (index, line) in lines.iter().enumerate().skip(body_start) {
        if let Some(heading) = line.strip_prefix("## ") {
            if let Some(heading) = current_heading.take() {
                sections.push(ParsedSection {
                    slug: slugify(&heading),
                    heading,
                    lines: current_lines,
                });
                current_lines = Vec::new();
            }
            current_heading = Some(heading.trim().to_string());
            continue;
        }
        if line.starts_with("# ") {
            continue;
        }
        if current_heading.is_some() {
            current_lines.push(LineEntry {
                line_number: index + 1,
                text: (*line).to_string(),
            });
        }
    }

    if let Some(heading) = current_heading.take() {
        sections.push(ParsedSection {
            slug: slugify(&heading),
            heading,
            lines: current_lines,
        });
    }

    if sections.is_empty() {
        let lines = lines
            .iter()
            .enumerate()
            .skip(body_start)
            .filter(|(_, line)| !line.starts_with("# "))
            .map(|(index, line)| LineEntry {
                line_number: index + 1,
                text: (*line).to_string(),
            })
            .collect::<Vec<_>>();
        if !lines.is_empty() {
            sections.push(ParsedSection {
                heading: document_title.to_string(),
                slug: slugify(document_title),
                lines,
            });
        }
    }

    sections
}

#[derive(Debug)]
struct ChunkParseResult {
    node: MemoryNode,
    supersedes: Vec<String>,
    id: String,
}

fn build_section_chunk(
    kind: MemoryNodeKind,
    source_path: &str,
    document_id: &str,
    document_title: &str,
    section: &ParsedSection,
    snapshot_value: &str,
    frontmatter: &Frontmatter,
) -> Result<Option<ChunkParseResult>> {
    let raw_text = join_chunk_lines(&section.lines, MemoryChunkKind::Section);
    let (text, supersedes) = extract_supersedes_directives(&raw_text);
    if text.is_empty() {
        return Ok(None);
    }
    let line_start = section
        .lines
        .first()
        .map(|line| line.line_number)
        .unwrap_or(0);
    let line_end = section
        .lines
        .last()
        .map(|line| line.line_number)
        .unwrap_or(line_start);
    let node = build_memory_node(
        kind,
        MemoryChunkKind::Section,
        source_path,
        document_id,
        document_title,
        &section.heading,
        &section.slug,
        1,
        &section.heading,
        &text,
        line_start,
        line_end,
        snapshot_value,
        frontmatter,
    );
    let id = node.id.clone();
    Ok(Some(ChunkParseResult {
        node,
        supersedes,
        id,
    }))
}

fn build_memory_node(
    kind: MemoryNodeKind,
    chunk_kind: MemoryChunkKind,
    source_path: &str,
    document_id: &str,
    document_title: &str,
    section_heading: &str,
    section_slug: &str,
    chunk_ordinal: usize,
    title: &str,
    text: &str,
    line_start: usize,
    line_end: usize,
    snapshot_value: &str,
    frontmatter: &Frontmatter,
) -> MemoryNode {
    MemoryNode {
        id: memory_node_id(kind, source_path, section_slug, chunk_kind, chunk_ordinal),
        kind,
        chunk_kind,
        title: title.to_string(),
        text: text.to_string(),
        source_path: source_path.to_string(),
        document_id: document_id.to_string(),
        document_title: document_title.to_string(),
        section_heading: section_heading.to_string(),
        section_slug: section_slug.to_string(),
        chunk_ordinal,
        line_start,
        line_end,
        snapshot_kind: "mtime_unix_seconds".to_string(),
        snapshot_value: snapshot_value.to_string(),
        source_updated: frontmatter.updated.clone(),
        source_scope: frontmatter.scope.clone(),
        source_owner: frontmatter.owner.clone(),
        source_status: frontmatter.status.clone(),
        tags: frontmatter.tags.clone(),
    }
}

fn parse_bullet_chunks(section: &ParsedSection) -> Vec<ParsedSection> {
    let mut chunks = Vec::new();
    let mut current = Vec::new();

    for line in &section.lines {
        if line.text.starts_with("- ") {
            if !current.is_empty() {
                chunks.push(ParsedSection {
                    heading: section.heading.clone(),
                    slug: section.slug.clone(),
                    lines: current,
                });
                current = Vec::new();
            }
            current.push(LineEntry {
                line_number: line.line_number,
                text: line.text.trim_start_matches("- ").to_string(),
            });
            continue;
        }

        if current.is_empty() {
            continue;
        }
        current.push(line.clone());
    }

    if !current.is_empty() {
        chunks.push(ParsedSection {
            heading: section.heading.clone(),
            slug: section.slug.clone(),
            lines: current,
        });
    }

    chunks
}

fn join_chunk_lines(lines: &[LineEntry], chunk_kind: MemoryChunkKind) -> String {
    let mut normalized = Vec::with_capacity(lines.len());
    for line in lines {
        let text = match chunk_kind {
            MemoryChunkKind::Section => line.text.trim_end().to_string(),
            MemoryChunkKind::Bullet => line
                .text
                .trim_start_matches("  ")
                .trim_start_matches('\t')
                .trim_end()
                .to_string(),
        };
        normalized.push(text);
    }
    normalized.join("\n").trim_matches('\n').trim().to_string()
}

fn extract_supersedes_directives(text: &str) -> (String, Vec<String>) {
    let mut supersedes = BTreeSet::new();
    let cleaned = supersedes_regex()
        .replace_all(text, |caps: &regex::Captures| {
            for raw_id in caps[1].split(|ch: char| ch == ',' || ch == ';') {
                let raw_id = raw_id.trim();
                if !raw_id.is_empty() {
                    supersedes.insert(raw_id.to_string());
                }
            }
            ""
        })
        .to_string();
    (cleaned.trim().to_string(), supersedes.into_iter().collect())
}

fn memory_node_id(
    kind: MemoryNodeKind,
    source_path: &str,
    section_slug: &str,
    chunk_kind: MemoryChunkKind,
    chunk_ordinal: usize,
) -> String {
    match chunk_kind {
        MemoryChunkKind::Section => {
            format!(
                "memory:{}:{}:{}",
                kind.id_prefix(),
                source_path,
                section_slug
            )
        }
        MemoryChunkKind::Bullet => format!(
            "memory:{}:{}:{}:{}",
            kind.id_prefix(),
            source_path,
            section_slug,
            chunk_ordinal
        ),
    }
}

fn infer_repo_root_from_memory_root(memory_state_root: &Path) -> Option<PathBuf> {
    memory_state_root
        .parent()?
        .parent()?
        .parent()
        .map(Path::to_path_buf)
}

fn repo_relative_path(path: &Path, repo_root: Option<&Path>) -> String {
    if let Some(repo_root) = repo_root {
        if let Ok(relative) = path.strip_prefix(repo_root) {
            return relative.to_string_lossy().replace('\\', "/");
        }
    }
    path.to_string_lossy().replace('\\', "/")
}

fn snapshot_value(path: &Path) -> Result<String> {
    let modified = fs::metadata(path)
        .with_context(|| format!("Failed to stat {}", path.display()))?
        .modified()
        .with_context(|| format!("Failed to read modified time for {}", path.display()))?;
    let duration = modified.duration_since(UNIX_EPOCH).with_context(|| {
        format!(
            "Modified time for {} predates the Unix epoch",
            path.display()
        )
    })?;
    Ok(duration.as_secs().to_string())
}

fn extract_reference_candidates(text: &str) -> Vec<ReferenceCandidate> {
    let mut candidates = BTreeSet::new();

    for captures in markdown_link_regex().captures_iter(text) {
        let target = captures[1].trim();
        if let Some(candidate) = classify_inline_reference(target) {
            candidates.insert(candidate);
        }
    }

    for captures in code_span_regex().captures_iter(text) {
        let raw = captures[1].trim();
        if let Some(candidate) = classify_inline_reference(raw) {
            candidates.insert(candidate);
        }
    }

    for captures in citation_key_regex().captures_iter(text) {
        candidates.insert(ReferenceCandidate::CitationKey(captures[1].to_string()));
    }

    for captures in arxiv_id_regex().captures_iter(text) {
        candidates.insert(ReferenceCandidate::ArxivId(captures[0].to_string()));
    }

    candidates.into_iter().collect()
}

fn classify_inline_reference(raw: &str) -> Option<ReferenceCandidate> {
    if raw.is_empty() {
        return None;
    }
    if let Some(citation_key) = raw.strip_prefix('@') {
        if !citation_key.is_empty() {
            return Some(ReferenceCandidate::CitationKey(citation_key.to_string()));
        }
    }
    if arxiv_id_regex().is_match(raw) {
        return Some(ReferenceCandidate::ArxivId(raw.to_string()));
    }
    if looks_like_path(raw) {
        return Some(ReferenceCandidate::Path(normalize_repo_path(raw)));
    }
    if looks_like_code_symbol(raw) {
        return Some(ReferenceCandidate::Symbol(raw.to_string()));
    }
    None
}

fn resolve_reference_candidate(
    candidate: &ReferenceCandidate,
    repo_root: Option<&Path>,
    paper_lookup: &PaperLookup,
) -> Option<ResolvedTarget> {
    match candidate {
        ReferenceCandidate::Path(path) => {
            if let Some(paper_id) = paper_lookup.resolve_path_stem(path) {
                return Some(ResolvedTarget::ExistingPaper {
                    id: format!("paper:{paper_id}"),
                    target_kind: "paper".to_string(),
                });
            }

            let kind = if is_doc_path(path) {
                MemorySurfaceKind::Doc
            } else if is_code_path(path) {
                MemorySurfaceKind::Code
            } else {
                return None;
            };
            Some(ResolvedTarget::Surface(build_path_surface(
                kind, path, repo_root,
            )))
        }
        ReferenceCandidate::Symbol(symbol) => {
            Some(ResolvedTarget::Surface(build_symbol_surface(symbol)))
        }
        ReferenceCandidate::CitationKey(citation_key) => {
            if let Some(paper_id) = paper_lookup.resolve_citation_key(citation_key) {
                return Some(ResolvedTarget::ExistingPaper {
                    id: format!("paper:{paper_id}"),
                    target_kind: "paper".to_string(),
                });
            }
            Some(ResolvedTarget::Surface(build_paper_reference_surface(
                format!("@{citation_key}"),
            )))
        }
        ReferenceCandidate::ArxivId(arxiv_id) => {
            if let Some(paper_id) = paper_lookup.resolve_arxiv_id(arxiv_id) {
                return Some(ResolvedTarget::ExistingPaper {
                    id: format!("paper:{paper_id}"),
                    target_kind: "paper".to_string(),
                });
            }
            Some(ResolvedTarget::Surface(build_paper_reference_surface(
                arxiv_id.clone(),
            )))
        }
    }
}

fn build_path_surface(
    kind: MemorySurfaceKind,
    path: &str,
    repo_root: Option<&Path>,
) -> MemorySurface {
    let exists = repo_root
        .map(|root| root.join(path).exists())
        .unwrap_or(false);
    MemorySurface {
        id: format!("{}:repo:{}", kind.id_prefix(), path),
        kind,
        locator: path.to_string(),
        repo_path: Some(path.to_string()),
        symbol: None,
        exists,
    }
}

fn build_symbol_surface(symbol: &str) -> MemorySurface {
    MemorySurface {
        id: format!("{}:symbol:{}", MemorySurfaceKind::Code.id_prefix(), symbol),
        kind: MemorySurfaceKind::Code,
        locator: symbol.to_string(),
        repo_path: None,
        symbol: Some(symbol.to_string()),
        exists: false,
    }
}

fn build_paper_reference_surface(locator: String) -> MemorySurface {
    let normalized = locator
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '@') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    MemorySurface {
        id: format!(
            "{}:ref:{}",
            MemorySurfaceKind::Paper.id_prefix(),
            normalized.trim_matches('-')
        ),
        kind: MemorySurfaceKind::Paper,
        locator,
        repo_path: None,
        symbol: None,
        exists: false,
    }
}

fn relation_types_for(kind: MemoryNodeKind, target_kind: &str) -> Vec<MemoryRelationType> {
    let mut relations = Vec::new();
    if target_kind == MemorySurfaceKind::Code.kind_name()
        && matches!(
            kind,
            MemoryNodeKind::ProjectState | MemoryNodeKind::Decision | MemoryNodeKind::Gotcha
        )
    {
        relations.push(MemoryRelationType::DocumentsCode);
    }
    if kind.is_constraint_source()
        && target_kind != "paper"
        && target_kind != MemorySurfaceKind::Paper.kind_name()
    {
        relations.push(MemoryRelationType::Constrains);
    }
    if relations.is_empty()
        || target_kind == "paper"
        || target_kind == MemorySurfaceKind::Paper.kind_name()
    {
        relations.push(MemoryRelationType::RelatesTo);
    }
    relations
}

fn add_relation(
    relation_map: &mut BTreeMap<(String, String, MemoryRelationType, String), BTreeSet<String>>,
    source_id: &str,
    target_id: &str,
    relation_type: MemoryRelationType,
    target_kind: &str,
    evidence: impl Into<String>,
) {
    relation_map
        .entry((
            source_id.to_string(),
            target_id.to_string(),
            relation_type,
            target_kind.to_string(),
        ))
        .or_default()
        .insert(evidence.into());
}

fn parse_inline_list(raw: &str) -> Vec<String> {
    raw.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn normalize_repo_path(raw: &str) -> String {
    raw.trim()
        .trim_matches(|ch| matches!(ch, '"' | '\''))
        .trim_start_matches("./")
        .replace('\\', "/")
}

fn looks_like_path(raw: &str) -> bool {
    let normalized = raw.trim();
    normalized == "AGENTS.md"
        || normalized == "README.md"
        || normalized == "Makefile"
        || normalized.starts_with(".agents/")
        || normalized.contains('/')
        || normalized.contains('\\')
        || matches!(
            Path::new(normalized)
                .extension()
                .and_then(|ext| ext.to_str()),
            Some(
                "md" | "qmd"
                    | "rst"
                    | "txt"
                    | "typ"
                    | "pdf"
                    | "bib"
                    | "rs"
                    | "py"
                    | "sh"
                    | "toml"
                    | "yaml"
                    | "yml"
                    | "json"
                    | "jsonl"
                    | "ipynb"
                    | "js"
                    | "ts"
                    | "tsx"
                    | "jsx"
            )
        )
}

fn looks_like_code_symbol(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.contains(' ') || trimmed.contains(':') || trimmed.contains('/') {
        return false;
    }
    dotted_identifier_regex().is_match(trimmed)
        || camel_case_symbol_regex().is_match(trimmed)
        || snake_case_symbol_regex().is_match(trimmed)
}

fn is_doc_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.starts_with("docs/")
        || lower.starts_with(".agents/")
        || matches!(
            Path::new(path).extension().and_then(|ext| ext.to_str()),
            Some("md" | "qmd" | "rst" | "txt" | "typ" | "pdf" | "bib")
        )
}

fn is_code_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower == "makefile"
        || lower.starts_with("src/")
        || lower.starts_with("crates/")
        || lower.starts_with("scripts/")
        || lower.starts_with("python/")
        || lower.starts_with("tests/")
        || lower.starts_with("examples/")
        || lower.starts_with("aria_nbv/")
        || matches!(
            Path::new(path).extension().and_then(|ext| ext.to_str()),
            Some(
                "rs" | "py"
                    | "sh"
                    | "toml"
                    | "yaml"
                    | "yml"
                    | "json"
                    | "jsonl"
                    | "ipynb"
                    | "js"
                    | "ts"
                    | "tsx"
                    | "jsx"
            )
        )
}

fn slugify(raw: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn markdown_link_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\[[^\]]+\]\(([^)]+)\)").unwrap())
}

fn code_span_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"`([^`\n]+)`").unwrap())
}

fn citation_key_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"@([A-Za-z][A-Za-z0-9:_-]+)").unwrap())
}

fn arxiv_id_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b\d{4}\.\d{4,5}(?:v\d+)?\b").unwrap())
}

fn supersedes_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"<!--\s*supersedes:\s*(.*?)\s*-->").unwrap())
}

fn dotted_identifier_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)+$").unwrap())
}

fn camel_case_symbol_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^[A-Z][A-Za-z0-9_]+$").unwrap())
}

fn snake_case_symbol_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^[a-z][a-z0-9_]*_[a-z0-9_]+$").unwrap())
}

impl ReferenceCandidate {
    fn raw_value(&self) -> String {
        match self {
            ReferenceCandidate::Path(value)
            | ReferenceCandidate::Symbol(value)
            | ReferenceCandidate::CitationKey(value)
            | ReferenceCandidate::ArxivId(value) => value.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DownloadMode, PaperSection, PaperSourceRecord, ParseStatus, SinkMode, SourceKind};

    fn config(root: &Path) -> RepoConfig {
        RepoConfig {
            manifest_path: root.join("sources.jsonl"),
            bib_path: root.join("references.bib"),
            tex_root: root.join("tex"),
            pdf_root: root.join("pdf"),
            generated_docs_root: root.join("generated"),
            registry_path: None,
            parsed_root: None,
            neo4j_export_root: None,
            memory_state_root: Some(root.join(".agents/memory/state")),
            sink: SinkMode::Neo4j,
            graphify_rebuild_command: None,
            download_pdfs: false,
            relevance_tags: vec![],
        }
    }

    fn sample_paper() -> ParsedPaper {
        ParsedPaper {
            metadata: PaperSourceRecord {
                paper_id: "efm3d-foundation".into(),
                citation_key: Some("efm3d2024".into()),
                title: "EFM3D".into(),
                authors: vec!["Author".into()],
                year: Some("2024".into()),
                arxiv_id: Some("2509.01584".into()),
                doi: None,
                url: None,
                tex_dir: None,
                pdf_file: None,
                source_kind: SourceKind::Bib,
                download_mode: DownloadMode::MetadataOnly,
                has_local_tex: false,
                has_local_pdf: false,
                parse_status: ParseStatus::MetadataOnly,
            },
            abstract_text: None,
            sections: vec![PaperSection {
                level: 1,
                title: "Intro".into(),
                content: "Foundation model paper.".into(),
            }],
            figures: vec![],
            tables: vec![],
            citations: vec![],
            provenance: vec![],
        }
    }

    #[test]
    fn imports_typed_memory_nodes_with_provenance_and_surface_links() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".agents/memory/state")).unwrap();
        fs::create_dir_all(root.join("docs/typst/paper")).unwrap();
        fs::create_dir_all(root.join("aria_nbv/aria_nbv/data_handling")).unwrap();
        fs::write(root.join("AGENTS.md"), "# Repo guidance\n").unwrap();
        fs::write(root.join("docs/typst/paper/main.typ"), "= Paper\n").unwrap();
        fs::write(
            root.join("aria_nbv/aria_nbv/data_handling/_legacy_cache_api.py"),
            "class Legacy: ...\n",
        )
        .unwrap();

        fs::write(
            root.join(".agents/memory/state/PROJECT_STATE.md"),
            r#"---
id: project_state
updated: 2026-04-13
scope: repo
owner: jan
status: active
tags: [nbv, codex]
---

# Project State

## Current Architecture
Training and diagnostics live in `aria_nbv/aria_nbv`, while the paper source of truth stays in `docs/typst/paper/main.typ`.
"#,
        )
        .unwrap();
        fs::write(
            root.join(".agents/memory/state/DECISIONS.md"),
            r#"---
id: decisions
updated: 2026-04-13
scope: repo
owner: jan
status: active
tags: [workflow]
---

# Decisions

## Durable Repo Decisions
- Keep the repo-root `AGENTS.md` thin and policy-only.
- Prefer the canonical owner `aria_nbv.data_handling` for cache contracts. <!-- supersedes: memory:decision:.agents/memory/state/DECISIONS.md:durable-repo-decisions:1 -->
"#,
        )
        .unwrap();
        fs::write(
            root.join(".agents/memory/state/OPEN_QUESTIONS.md"),
            r#"---
id: open_questions
updated: 2026-03-24
scope: repo
owner: jan
status: active
tags: [research]
---

# Open Questions

## Research Questions
- Which findings from @efm3d2024 matter most for 2509.01584?
"#,
        )
        .unwrap();
        fs::write(
            root.join(".agents/memory/state/GOTCHAS.md"),
            r#"---
id: gotchas
updated: 2026-03-30
scope: repo
owner: jan
status: active
tags: [frames]
---

# Gotchas

## Frames and Geometry
- Use `PoseTW` and `CameraTW` instead of raw matrices in normal package code.
"#,
        )
        .unwrap();

        let bundle = load_project_memory(&config(root), &[sample_paper()]).unwrap();

        assert_eq!(
            bundle
                .nodes
                .iter()
                .filter(|node| node.kind == MemoryNodeKind::ProjectState)
                .count(),
            1
        );
        assert_eq!(
            bundle
                .nodes
                .iter()
                .filter(|node| node.kind == MemoryNodeKind::Decision)
                .count(),
            2
        );
        assert_eq!(
            bundle
                .nodes
                .iter()
                .filter(|node| node.kind == MemoryNodeKind::OpenQuestion)
                .count(),
            1
        );
        assert_eq!(
            bundle
                .nodes
                .iter()
                .filter(|node| node.kind == MemoryNodeKind::Gotcha)
                .count(),
            1
        );

        let architecture = bundle
            .nodes
            .iter()
            .find(|node| node.kind == MemoryNodeKind::ProjectState)
            .unwrap();
        assert_eq!(
            architecture.source_path,
            ".agents/memory/state/PROJECT_STATE.md"
        );
        assert_eq!(architecture.section_heading, "Current Architecture");
        assert!(architecture.line_start > 0);
        assert!(architecture.line_end >= architecture.line_start);
        assert_eq!(architecture.snapshot_kind, "mtime_unix_seconds");
        assert!(!architecture.snapshot_value.is_empty());

        assert!(bundle
            .surfaces
            .iter()
            .any(|surface| surface.id == "doc_surface:repo:docs/typst/paper/main.typ"));
        assert!(bundle
            .surfaces
            .iter()
            .any(|surface| surface.id == "code_surface:repo:aria_nbv/aria_nbv"));
        assert!(bundle
            .surfaces
            .iter()
            .any(|surface| surface.id == "doc_surface:repo:AGENTS.md"));
        assert!(bundle
            .surfaces
            .iter()
            .any(|surface| surface.id == "code_surface:symbol:aria_nbv.data_handling"));
        assert!(bundle
            .surfaces
            .iter()
            .any(|surface| surface.id == "code_surface:symbol:PoseTW"));

        assert!(bundle.relations.iter().any(|relation| {
            relation.relation_type == MemoryRelationType::DocumentsCode
                && relation.target_kind == "code_surface"
        }));
        assert!(bundle.relations.iter().any(|relation| {
            relation.relation_type == MemoryRelationType::Constrains
                && relation.target_id == "doc_surface:repo:AGENTS.md"
        }));
        assert!(bundle.relations.iter().any(|relation| {
            relation.relation_type == MemoryRelationType::RelatesTo
                && relation.target_id == "paper:efm3d-foundation"
        }));
        assert!(bundle.relations.iter().any(|relation| {
            relation.relation_type == MemoryRelationType::Supersedes
                && relation.target_id
                    == "memory:decision:.agents/memory/state/DECISIONS.md:durable-repo-decisions:1"
        }));
    }
}
