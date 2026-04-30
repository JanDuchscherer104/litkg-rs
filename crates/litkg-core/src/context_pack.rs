use crate::config::RepoConfig;
use crate::materialize::load_parsed_papers;
use crate::model::{PaperSourceRecord, ParsedPaper};
use crate::registry::load_registry;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ContextPackRequest {
    pub repo_root: PathBuf,
    pub task: String,
    pub budget_tokens: usize,
    pub profile: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextPack {
    pub task: String,
    pub profile: String,
    pub budget_tokens: usize,
    pub truncated: bool,
    pub task_summary: String,
    pub active_issues: Vec<ContextBacklogItem>,
    pub active_todos: Vec<ContextBacklogItem>,
    pub evidence_spans: Vec<ContextEvidenceSpan>,
    pub relevant_papers: Vec<ContextPaper>,
    pub missing_context_leaves: Vec<MissingContextLeaf>,
    pub verification_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextBacklogItem {
    pub id: String,
    pub title: String,
    pub priority: String,
    pub status: String,
    pub summary: String,
    pub issue_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextEvidenceSpan {
    pub source_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub kind: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextPaper {
    pub paper_id: String,
    pub citation_key: Option<String>,
    pub title: String,
    pub year: Option<String>,
    pub matched_terms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MissingContextLeaf {
    pub provider: String,
    pub query: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
struct IssuesToml {
    #[serde(default)]
    issues: Vec<IssueToml>,
}

#[derive(Debug, Deserialize)]
struct IssueToml {
    id: String,
    title: String,
    summary: String,
    priority: String,
    status: String,
}

#[derive(Debug, Deserialize)]
struct TodosToml {
    #[serde(default)]
    todos: Vec<TodoToml>,
}

#[derive(Debug, Deserialize)]
struct TodoToml {
    id: String,
    title: String,
    #[serde(default)]
    issue_ids: Vec<String>,
    priority: String,
    status: String,
}

pub fn build_context_pack(config: &RepoConfig, request: ContextPackRequest) -> Result<ContextPack> {
    if request.profile != "agents-scaffold" {
        bail!(
            "unsupported context-pack profile '{}'; expected agents-scaffold",
            request.profile
        );
    }
    if request.task.trim().is_empty() {
        bail!("--task must not be empty");
    }
    if request.budget_tokens == 0 {
        bail!("--budget must be at least 1");
    }

    let repo_root = request.repo_root;
    let task_terms = task_terms(&request.task);
    let active_issues = load_active_issues(&repo_root)?;
    let active_todos = load_active_todos(&repo_root)?;
    let mut evidence_spans = collect_evidence_spans(&repo_root, &task_terms)?;
    let relevant_papers = collect_relevant_papers(config, &task_terms)?;
    let mut truncated = false;

    truncate_spans_to_budget(&mut evidence_spans, request.budget_tokens, &mut truncated);

    Ok(ContextPack {
        task_summary: format!("Context pack for: {}", request.task.trim()),
        task: request.task,
        profile: request.profile,
        budget_tokens: request.budget_tokens,
        truncated,
        active_issues,
        active_todos,
        evidence_spans,
        relevant_papers,
        missing_context_leaves: vec![
            MissingContextLeaf {
                provider: "Context7".into(),
                query: "Resolve current library docs relevant to the task.".into(),
                status: "pending".into(),
            },
            MissingContextLeaf {
                provider: "openaiDeveloperDocs".into(),
                query: "Resolve current OpenAI/Codex/MCP docs relevant to the task.".into(),
                status: "pending".into(),
            },
        ],
        verification_commands: vec![
            "make agents-db-check".into(),
            "make agents-db AGENTS_ARGS='validate'".into(),
            "cargo fmt --all --check".into(),
            "cargo test --all-features".into(),
        ],
    })
}

fn task_terms(task: &str) -> BTreeSet<String> {
    task.split(|c: char| !c.is_ascii_alphanumeric())
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| term.len() >= 3)
        .collect()
}

fn load_active_issues(repo_root: &Path) -> Result<Vec<ContextBacklogItem>> {
    let path = repo_root.join(".agents/issues.toml");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let data: IssuesToml =
        toml::from_str(&raw).with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(data
        .issues
        .into_iter()
        .filter(|issue| issue.status != "closed")
        .map(|issue| ContextBacklogItem {
            id: issue.id,
            title: issue.title,
            priority: issue.priority,
            status: issue.status,
            summary: issue.summary,
            issue_ids: Vec::new(),
        })
        .collect())
}

fn load_active_todos(repo_root: &Path) -> Result<Vec<ContextBacklogItem>> {
    let path = repo_root.join(".agents/todos.toml");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let data: TodosToml =
        toml::from_str(&raw).with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(data
        .todos
        .into_iter()
        .filter(|todo| todo.status != "done")
        .map(|todo| ContextBacklogItem {
            id: todo.id,
            title: todo.title,
            priority: todo.priority,
            status: todo.status,
            summary: String::new(),
            issue_ids: todo.issue_ids,
        })
        .collect())
}

fn collect_evidence_spans(
    repo_root: &Path,
    terms: &BTreeSet<String>,
) -> Result<Vec<ContextEvidenceSpan>> {
    let mut paths = vec![
        repo_root.join("AGENTS.md"),
        repo_root.join("README.md"),
        repo_root.join("docs/architecture.md"),
        repo_root.join(".agents/AGENTS_INTERNAL_DB.md"),
        repo_root.join(".agents/issues.toml"),
        repo_root.join(".agents/todos.toml"),
        repo_root.join(".agents/resolved.toml"),
    ];
    let skills_root = repo_root.join(".agents/skills");
    if skills_root.exists() {
        let mut skill_paths = fs::read_dir(&skills_root)?
            .flatten()
            .map(|entry| entry.path().join("SKILL.md"))
            .filter(|path| path.exists())
            .collect::<Vec<_>>();
        skill_paths.sort();
        paths.extend(skill_paths);
    }

    let mut spans = Vec::new();
    for path in paths.into_iter().filter(|path| path.exists()) {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let source_path = relative_path(repo_root, &path);
        spans.extend(select_spans(&source_path, &raw, terms));
    }
    spans.sort_by(|left, right| {
        left.source_path
            .cmp(&right.source_path)
            .then(left.line_start.cmp(&right.line_start))
    });
    Ok(spans)
}

fn select_spans(
    source_path: &str,
    raw: &str,
    terms: &BTreeSet<String>,
) -> Vec<ContextEvidenceSpan> {
    let lines = raw.lines().collect::<Vec<_>>();
    let mut spans = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let line_lower = line.to_ascii_lowercase();
        if terms.iter().any(|term| line_lower.contains(term)) {
            let start = index.saturating_sub(1);
            let end = usize::min(index + 2, lines.len());
            let text = lines[start..end].join("\n");
            spans.push(ContextEvidenceSpan {
                source_path: source_path.into(),
                line_start: start + 1,
                line_end: end,
                kind: "matched_text".into(),
                text,
            });
            if spans.len() >= 4 {
                return spans;
            }
        }
    }
    if spans.is_empty() && !lines.is_empty() {
        let end = usize::min(6, lines.len());
        spans.push(ContextEvidenceSpan {
            source_path: source_path.into(),
            line_start: 1,
            line_end: end,
            kind: "fallback_intro".into(),
            text: lines[..end].join("\n"),
        });
    }
    spans
}

fn collect_relevant_papers(
    config: &RepoConfig,
    terms: &BTreeSet<String>,
) -> Result<Vec<ContextPaper>> {
    let registry = if config.registry_path().exists() {
        load_registry(config.registry_path())?
    } else {
        Vec::new()
    };
    let parsed = load_parsed_papers(config.parsed_root())?;
    let mut candidates = registry
        .iter()
        .map(|record| paper_from_record(record, terms))
        .collect::<Vec<_>>();
    candidates.extend(parsed.iter().map(|paper| paper_from_parsed(paper, terms)));
    candidates.retain(|(_, paper)| !paper.matched_terms.is_empty());
    candidates.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(left.1.paper_id.cmp(&right.1.paper_id))
    });
    candidates.dedup_by(|left, right| left.1.paper_id == right.1.paper_id);
    Ok(candidates
        .into_iter()
        .take(5)
        .map(|(_, paper)| paper)
        .collect())
}

fn paper_from_record(
    record: &PaperSourceRecord,
    terms: &BTreeSet<String>,
) -> (usize, ContextPaper) {
    let haystack = format!(
        "{} {} {}",
        record.title,
        record.authors.join(" "),
        record.citation_key.clone().unwrap_or_default()
    );
    let matched_terms = matched_terms(&haystack, terms);
    (
        matched_terms.len(),
        ContextPaper {
            paper_id: record.paper_id.clone(),
            citation_key: record.citation_key.clone(),
            title: record.title.clone(),
            year: record.year.clone(),
            matched_terms,
        },
    )
}

fn paper_from_parsed(paper: &ParsedPaper, terms: &BTreeSet<String>) -> (usize, ContextPaper) {
    let section_text = paper
        .sections
        .iter()
        .map(|section| format!("{} {}", section.title, section.content))
        .collect::<Vec<_>>()
        .join(" ");
    let haystack = format!(
        "{} {} {}",
        paper.metadata.title,
        paper.abstract_text.clone().unwrap_or_default(),
        section_text
    );
    let matched_terms = matched_terms(&haystack, terms);
    (
        matched_terms.len(),
        ContextPaper {
            paper_id: paper.metadata.paper_id.clone(),
            citation_key: paper.metadata.citation_key.clone(),
            title: paper.metadata.title.clone(),
            year: paper.metadata.year.clone(),
            matched_terms,
        },
    )
}

fn matched_terms(haystack: &str, terms: &BTreeSet<String>) -> Vec<String> {
    let lower = haystack.to_ascii_lowercase();
    terms
        .iter()
        .filter(|term| lower.contains(term.as_str()))
        .cloned()
        .collect()
}

fn truncate_spans_to_budget(
    spans: &mut Vec<ContextEvidenceSpan>,
    budget_tokens: usize,
    truncated: &mut bool,
) {
    let mut used = 0usize;
    let mut keep = 0usize;
    for span in spans.iter() {
        let token_estimate = span.text.split_whitespace().count() + 8;
        if used + token_estimate > budget_tokens {
            *truncated = true;
            break;
        }
        used += token_estimate;
        keep += 1;
    }
    spans.truncate(keep.max(1).min(spans.len()));
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
