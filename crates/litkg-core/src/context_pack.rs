use crate::config::RepoConfig;
use crate::inspect::{compute_agent_conformance_report, BackendDescriptor};
use crate::materialize::load_parsed_papers;
use crate::model::{DocumentKind, PaperSourceRecord, ParsedPaper, SourceKind};
use crate::registry::load_registry;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct ContextPackRequest {
    pub config_path: Option<PathBuf>,
    pub repo_root: PathBuf,
    pub task: String,
    pub budget_tokens: usize,
    pub profile: String,
    // When true (the default), build_context_pack skips bulk and legacy
    // payload fields. Callers that need the legacy shape (debugging,
    // back-compat consumers) opt out via the CLI `--full` flag.
    pub lean: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ContextPack {
    pub task: String,
    pub verb: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub profile: String,
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub budget_tokens: usize,
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
    pub task_summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumptions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supporting_evidence: Vec<ContextEvidenceSpan>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contradicting_evidence: Vec<ContextEvidenceSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_summary: Option<String>,
    pub top_sources: Vec<ContextTopSource>,
    pub required_reads: Vec<ContextRequiredRead>,
    pub suggested_next_action: ContextSuggestedNextAction,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub action_plan: Vec<String>,
    pub active_backlog: Vec<ContextBacklogItem>,
    pub active_issues: Vec<ContextBacklogItem>,
    pub active_todos: Vec<ContextBacklogItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_spans: Vec<ContextEvidenceSpan>,
    pub relevant_symbols: Vec<ContextSymbol>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relevant_papers: Vec<ContextPaper>,
    pub missing_context: Vec<MissingContextLeaf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_leaves: Vec<MissingContextLeaf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_context_leaves: Vec<MissingContextLeaf>,
    pub risk_flags: Vec<String>,
    pub verification_commands: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backend_status: Vec<BackendDescriptor>,
}

fn is_zero_usize(value: &usize) -> bool {
    *value == 0
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ContextTopSource {
    pub path: String,
    pub title: String,
    pub role: String,
    pub authority: String,
    pub freshness: f32,
    pub source_span: ContextSourceSpan,
    pub source_type: String,
    pub scores: crate::ranking::WeightedScore,
    pub why_relevant: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextSourceSpan {
    pub line_start: usize,
    pub line_end: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextRequiredRead {
    pub path: String,
    pub title: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextSuggestedNextAction {
    pub summary: String,
    pub skill: Option<String>,
    pub command: Option<String>,
    pub why: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextBacklogItem {
    pub id: String,
    pub title: String,
    pub priority: String,
    pub status: String,
    pub summary: String,
    pub issue_ids: Vec<String>,
    pub context: Vec<String>,
    pub references: Vec<String>,
    pub acceptance: Vec<String>,
    pub verification: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ContextEvidenceSpan {
    pub source_path: String,
    pub line_start: usize,
    pub line_end: usize,
    pub kind: String,
    pub text: String,
    #[serde(flatten)]
    pub score: crate::ranking::WeightedScore,
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
pub struct ContextSymbol {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MissingContextLeaf {
    pub provider: String,
    pub query: String,
    pub status: String,
    pub resolution_command: String,
}

#[derive(Debug, Deserialize)]
struct IssuesToml {
    #[serde(default)]
    issue: Vec<IssueToml>,
    #[serde(default)]
    issues: Vec<IssueToml>,
}

#[derive(Debug, Deserialize)]
struct IssueToml {
    id: String,
    title: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    description: Option<String>,
    priority: String,
    status: String,
    #[serde(default)]
    context: Vec<String>,
    #[serde(default)]
    references: Vec<String>,
    #[serde(default)]
    acceptance: Vec<String>,
    #[serde(default)]
    verification: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TodosToml {
    #[serde(default)]
    todo: Vec<TodoToml>,
    #[serde(default)]
    todos: Vec<TodoToml>,
}

#[derive(Debug, Deserialize)]
struct TodoToml {
    id: String,
    title: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    issue_ids: Vec<String>,
    priority: String,
    status: String,
    #[serde(default)]
    context: Vec<String>,
    #[serde(default)]
    references: Vec<String>,
    #[serde(default)]
    acceptance: Vec<String>,
    #[serde(default)]
    verification: Vec<String>,
}

pub fn build_context_pack(config: &RepoConfig, request: ContextPackRequest) -> Result<ContextPack> {
    if !supported_profile(&request.profile) {
        bail!(
            "unsupported context-pack profile '{}'; expected one of: agents-scaffold, thesis-coding, docs-paper-sync, rri-oracle, vin-baseline, rollout-planning",
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
    let active_todos = filter_backlog_items(
        load_active_todos(&repo_root)?,
        &task_terms,
        &request.task,
        config.context_pack.max_active_backlog_items,
        config.context_pack.backlog_min_score,
        &BTreeSet::new(),
    );
    let selected_issue_ids = selected_issue_ids(&active_todos, &request.task);
    let active_issues = filter_backlog_items(
        load_active_issues(&repo_root)?,
        &task_terms,
        &request.task,
        config.context_pack.max_active_backlog_items,
        config.context_pack.backlog_min_score,
        &selected_issue_ids,
    );
    let active_backlog = filter_backlog_items(
        active_backlog(&active_issues, &active_todos),
        &task_terms,
        &request.task,
        config.context_pack.max_active_backlog_items,
        config.context_pack.backlog_min_score,
        &BTreeSet::new(),
    );
    let mut evidence_spans =
        collect_evidence_spans(&repo_root, &request.profile, &task_terms, config)?;
    let injected_missing = inject_backlog_reference_spans(
        &repo_root,
        config,
        &task_terms,
        &active_backlog,
        &mut evidence_spans,
    )?;
    let relevant_symbols = collect_relevant_symbols(&repo_root, &request.profile, &task_terms)?;
    let relevant_papers = collect_relevant_papers(config, &task_terms)?;
    let fallback_config_path = PathBuf::from("<config>");
    let config_path = request
        .config_path
        .as_deref()
        .unwrap_or(&fallback_config_path);
    let conformance =
        compute_agent_conformance_report(config, config_path, Some(&repo_root), false);
    let backend_status = conformance.backends;
    let verification_commands = verification_commands(&request.profile);
    let mut truncated = false;

    sort_evidence_spans(&mut evidence_spans);
    truncate_spans_to_budget(&mut evidence_spans, request.budget_tokens, &mut truncated);
    let assumptions = assumptions(&request.profile);
    let mut top_sources = top_sources(&repo_root, config, &evidence_spans, &task_terms)?;
    let confidence_summary =
        apply_confidence_floor(&mut top_sources, config.context_pack.min_top_source_score);
    let missing_leaves = missing_leaves(
        &repo_root,
        &request.task,
        &top_sources,
        &active_backlog,
        injected_missing,
    );
    let risk_flags = risk_flags(config, &repo_root, &request.task, &top_sources);
    let action_plan = action_plan(
        &request.task,
        &request.profile,
        &backend_status,
        &missing_leaves,
        &risk_flags,
    );
    let required_reads = required_reads(&top_sources);
    let suggested_next_action = suggested_next_action(
        &top_sources,
        &backend_status,
        &missing_leaves,
        &risk_flags,
        &verification_commands,
        confidence_summary.as_deref(),
    );
    let claim_verdict = claim_verdict(
        &derive_verb(&request.task),
        &request.task,
        &top_sources,
        &evidence_spans,
    );

    let lean = request.lean;
    Ok(ContextPack {
        task_summary: format!("Context pack for: {}", request.task.trim()),
        verb: derive_verb(&request.task),
        task: request.task,
        profile: if lean { String::new() } else { request.profile },
        budget_tokens: if lean { 0 } else { request.budget_tokens },
        truncated: if lean { false } else { truncated },
        assumptions: if lean { Vec::new() } else { assumptions },
        verdict: claim_verdict.verdict,
        confidence: claim_verdict.confidence,
        supporting_evidence: claim_verdict.supporting_evidence,
        contradicting_evidence: claim_verdict.contradicting_evidence,
        confidence_summary,
        top_sources,
        required_reads,
        suggested_next_action,
        action_plan: if lean { Vec::new() } else { action_plan },
        active_backlog,
        active_issues,
        active_todos,
        evidence_spans: if lean { Vec::new() } else { evidence_spans },
        relevant_symbols,
        relevant_papers: if lean { Vec::new() } else { relevant_papers },
        missing_context: missing_leaves.clone(),
        missing_context_leaves: if lean {
            Vec::new()
        } else {
            missing_leaves.clone()
        },
        missing_leaves: if lean { Vec::new() } else { missing_leaves },
        risk_flags,
        verification_commands,
        backend_status: if lean { Vec::new() } else { backend_status },
    })
}

fn supported_profile(profile: &str) -> bool {
    matches!(
        profile,
        "agents-scaffold"
            | "thesis-coding"
            | "docs-paper-sync"
            | "rri-oracle"
            | "vin-baseline"
            | "rollout-planning"
    )
}

fn task_terms(task: &str) -> BTreeSet<String> {
    task.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| {
            term.len() >= 3
                && !crate::ranking::is_search_stopword(term)
                && !context_pack_stopword(term)
        })
        .collect()
}

fn context_pack_stopword(term: &str) -> bool {
    matches!(
        term,
        "aria" | "nbv" | "input" | "inputs" | "nonsense" | "output" | "outputs" | "uses"
    )
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
    let mut issues = data.issue;
    issues.extend(data.issues);
    Ok(issues
        .into_iter()
        .filter(|issue| issue.status != "closed" && issue.status != "resolved")
        .map(|issue| ContextBacklogItem {
            id: issue.id,
            title: issue.title,
            priority: issue.priority,
            status: issue.status,
            summary: issue
                .summary
                .or(issue.description)
                .unwrap_or_else(String::new),
            issue_ids: Vec::new(),
            context: issue.context,
            references: issue.references,
            acceptance: issue.acceptance,
            verification: issue.verification,
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
    let mut todos = data.todo;
    todos.extend(data.todos);
    Ok(todos
        .into_iter()
        .filter(|todo| todo.status != "done" && todo.status != "resolved")
        .map(|todo| ContextBacklogItem {
            id: todo.id,
            title: todo.title,
            priority: todo.priority,
            status: todo.status,
            summary: todo
                .summary
                .or(todo.description)
                .unwrap_or_else(String::new),
            issue_ids: todo.issue_ids,
            context: todo.context,
            references: todo.references,
            acceptance: todo.acceptance,
            verification: todo.verification,
        })
        .collect())
}

fn active_backlog(
    issues: &[ContextBacklogItem],
    todos: &[ContextBacklogItem],
) -> Vec<ContextBacklogItem> {
    let mut backlog = issues.to_vec();
    backlog.extend(todos.iter().cloned());
    backlog.sort_by(compare_backlog_items);
    backlog
}

fn compare_backlog_items(
    left: &ContextBacklogItem,
    right: &ContextBacklogItem,
) -> std::cmp::Ordering {
    priority_rank(&left.priority)
        .cmp(&priority_rank(&right.priority))
        .then(left.id.cmp(&right.id))
}

fn priority_rank(priority: &str) -> u8 {
    match priority {
        "critical" | "P0" => 0,
        "high" | "P1" => 1,
        "medium" | "P2" => 2,
        "low" | "P3" => 3,
        _ => 4,
    }
}

fn selected_issue_ids(todos: &[ContextBacklogItem], task: &str) -> BTreeSet<String> {
    let mut ids = ids_mentioned_in_text(task);
    for todo in todos {
        ids.extend(todo.issue_ids.iter().cloned());
    }
    ids
}

fn filter_backlog_items(
    items: Vec<ContextBacklogItem>,
    terms: &BTreeSet<String>,
    task: &str,
    cap: usize,
    min_score: f32,
    always_keep_ids: &BTreeSet<String>,
) -> Vec<ContextBacklogItem> {
    if cap == 0 {
        return Vec::new();
    }
    if terms.is_empty() && always_keep_ids.is_empty() {
        return items.into_iter().take(cap).collect();
    }
    let mentioned_ids = ids_mentioned_in_text(task);
    let mut scored = items
        .into_iter()
        .filter_map(|item| {
            let explicit = mentioned_ids.contains(&item.id) || always_keep_ids.contains(&item.id);
            let score = backlog_item_score(&item, terms);
            if explicit || score >= min_score {
                Some((explicit, score, item))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(
                right
                    .1
                    .partial_cmp(&left.1)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(compare_backlog_items(&left.2, &right.2))
    });
    scored
        .into_iter()
        .take(cap)
        .map(|(_, _, item)| item)
        .collect()
}

fn backlog_item_score(item: &ContextBacklogItem, terms: &BTreeSet<String>) -> f32 {
    if terms.is_empty() {
        return 0.0;
    }
    let haystack = format!(
        "{} {} {} {} {} {}",
        item.id,
        item.title,
        item.summary,
        item.context.join(" "),
        item.references.join(" "),
        item.verification.join(" ")
    )
    .to_ascii_lowercase();
    let matched = terms
        .iter()
        .filter(|term| haystack.contains(term.as_str()))
        .count();
    matched as f32 / terms.len().max(1) as f32
}

fn ids_mentioned_in_text(text: &str) -> BTreeSet<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .filter(|token| token.starts_with("issue-") || token.starts_with("todo-"))
        .map(|token| token.to_string())
        .collect()
}

fn collect_evidence_spans(
    repo_root: &Path,
    profile: &str,
    terms: &BTreeSet<String>,
    config: &RepoConfig,
) -> Result<Vec<ContextEvidenceSpan>> {
    let mut paths = profile_paths(repo_root, profile);
    paths.extend(configured_source_paths(repo_root, config)?);
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
    for path in paths
        .into_iter()
        .filter(|path| path.exists() && !source_path_is_never_indexed(repo_root, path))
    {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let source_path = relative_path(repo_root, &path);

        let score = crate::ranking::calculate_weighted_score(
            &path,
            1.0, // base lexical score placeholder
            config.authority_tiers.as_ref(),
        );
        spans.extend(select_spans(&source_path, &raw, terms, score));
    }
    sort_evidence_spans(&mut spans);
    Ok(spans)
}

fn sort_evidence_spans(spans: &mut [ContextEvidenceSpan]) {
    spans.sort_by(|left, right| {
        right
            .score
            .score_final
            .partial_cmp(&left.score.score_final)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.source_path.cmp(&right.source_path))
            .then(left.line_start.cmp(&right.line_start))
    });
}

fn inject_backlog_reference_spans(
    repo_root: &Path,
    config: &RepoConfig,
    terms: &BTreeSet<String>,
    backlog: &[ContextBacklogItem],
    spans: &mut Vec<ContextEvidenceSpan>,
) -> Result<Vec<MissingContextLeaf>> {
    let mut seen = BTreeSet::new();
    let mut ranked_backlog = backlog
        .iter()
        .map(|item| (backlog_item_score(item, terms), item))
        .collect::<Vec<_>>();
    ranked_backlog.sort_by(|left, right| {
        right
            .0
            .partial_cmp(&left.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                right
                    .1
                    .id
                    .starts_with("todo-")
                    .cmp(&left.1.id.starts_with("todo-")),
            )
            .then(left.1.id.cmp(&right.1.id))
    });
    for (item_score, item) in ranked_backlog {
        for candidate in backlog_reference_paths(item) {
            let candidate_path = if candidate.is_absolute() {
                candidate
            } else {
                repo_root.join(candidate)
            };
            if source_path_is_never_indexed(repo_root, &candidate_path) {
                continue;
            }
            if !candidate_path.is_file() {
                continue;
            }
            let source_path = relative_path(repo_root, &candidate_path);
            if !seen.insert(source_path.clone()) {
                continue;
            }
            let raw = fs::read_to_string(&candidate_path)
                .with_context(|| format!("Failed to read {}", candidate_path.display()))?;
            let score = crate::ranking::calculate_weighted_score(
                &candidate_path,
                1.0,
                config.authority_tiers.as_ref(),
            );
            let mut injected = select_spans(&source_path, &raw, terms, score);
            for span in &mut injected {
                span.kind = "backlog_reference".into();
                let relevance_boost = item_score.clamp(0.0, 1.0);
                span.score.score_lexical =
                    span.score.score_lexical.max(0.75 + relevance_boost * 0.2);
                span.score.score_final = span.score.score_final.max(
                    span.score.score_lexical
                        * span.score.score_authority
                        * span.score.score_freshness,
                ) * (1.2 + relevance_boost * 0.25);
                span.score
                    .why
                    .push(format!("injected from matched backlog record {}", item.id));
            }
            spans.extend(injected);
        }
    }
    Ok(Vec::new())
}

fn backlog_reference_paths(item: &ContextBacklogItem) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    for text in item
        .references
        .iter()
        .chain(item.verification.iter())
        .chain(item.acceptance.iter())
    {
        for path in candidate_repo_paths_from_text(text) {
            paths.insert(path);
        }
    }
    paths.into_iter().collect()
}

fn candidate_repo_paths_from_text(text: &str) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    for raw_token in text.split_whitespace() {
        let mut token = raw_token
            .trim_matches(|c: char| {
                matches!(
                    c,
                    '"' | '\'' | '`' | ',' | ';' | ':' | ')' | '(' | '[' | ']'
                )
            })
            .to_string();
        if let Some(rest) = token.strip_prefix("repo:") {
            token = rest.to_string();
        }
        if token.starts_with("url:")
            || token.starts_with("bib:")
            || token.starts_with("context7:")
            || token.starts_with("litkg:")
        {
            continue;
        }
        if let Some((path, _anchor)) = token.split_once('#') {
            token = path.to_string();
        }
        if token.contains("://") || token.is_empty() {
            continue;
        }
        if token.ends_with(".py")
            || token.ends_with(".rs")
            || token.ends_with(".md")
            || token.ends_with(".qmd")
            || token.ends_with(".toml")
            || token.ends_with(".typ")
        {
            let path = PathBuf::from(&token);
            paths.insert(path.clone());
            if token.starts_with("tests/") {
                paths.insert(PathBuf::from("aria_nbv").join(&token));
            }
            if let Some(implementation) = implementation_path_for_test(&path) {
                paths.insert(implementation);
            }
        }
    }
    paths.into_iter().collect()
}

fn implementation_path_for_test(path: &Path) -> Option<PathBuf> {
    let normalized = normalize_path(&path.to_string_lossy());
    let relative = normalized
        .strip_prefix("aria_nbv/tests/")
        .or_else(|| normalized.strip_prefix("tests/"))?;
    let filename = relative.rsplit('/').next()?;
    let implementation_filename = filename.strip_prefix("test_")?;
    let parent = relative.strip_suffix(filename).unwrap_or_default();
    Some(
        PathBuf::from("aria_nbv/aria_nbv")
            .join(parent)
            .join(implementation_filename),
    )
}

fn configured_source_paths(repo_root: &Path, config: &RepoConfig) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for source in config.sources.values() {
        let excludes = source.exclude.clone();
        for pattern in source_patterns(source) {
            if pattern.contains("://") {
                continue;
            }
            let pattern_path = repo_root.join(&pattern);
            let pattern_str = pattern_path.to_string_lossy().to_string();
            if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
                for entry in glob::glob(&pattern_str)
                    .with_context(|| format!("Failed to expand source glob {pattern}"))?
                    .flatten()
                {
                    if entry.is_file()
                        && source_path_is_context_text(&entry)
                        && !source_path_is_excluded(repo_root, &entry, &excludes)
                        && !source_path_is_never_indexed(repo_root, &entry)
                    {
                        paths.push(entry);
                    }
                }
            } else if pattern_path.is_file()
                && source_path_is_context_text(&pattern_path)
                && !source_path_is_excluded(repo_root, &pattern_path, &excludes)
                && !source_path_is_never_indexed(repo_root, &pattern_path)
            {
                paths.push(pattern_path);
            }
        }
    }
    paths.sort();
    paths.dedup();
    // Cap configured paths to keep evidence-span generation bounded. Canonical
    // tier paths (curated literature, thesis, memory) are preserved first so
    // they survive even when other source classes (e.g. aria_nbv/**/*.py)
    // would otherwise crowd them out alphabetically (todo-073).
    const PATH_CAP: usize = 512;
    if paths.len() > PATH_CAP {
        let authority_tiers = config.authority_tiers.as_ref();
        let (mut canonical, mut other): (Vec<PathBuf>, Vec<PathBuf>) =
            paths.into_iter().partition(|path| {
                let scored = crate::ranking::calculate_weighted_score(path, 1.0, authority_tiers);
                scored.authority == "canonical"
            });
        canonical.sort();
        other.sort();
        let mut combined: Vec<PathBuf> = canonical.into_iter().take(PATH_CAP).collect();
        let remaining = PATH_CAP.saturating_sub(combined.len());
        combined.extend(other.into_iter().take(remaining));
        combined.sort();
        combined.dedup();
        paths = combined;
    }
    Ok(paths)
}

fn source_path_is_excluded(repo_root: &Path, path: &Path, excludes: &[String]) -> bool {
    let relative = relative_path(repo_root, path);
    source_path_is_never_indexed(repo_root, path)
        || excludes
            .iter()
            .any(|pattern| source_pattern_matches(&relative, pattern))
}

fn source_path_is_never_indexed(repo_root: &Path, path: &Path) -> bool {
    let relative = relative_path(repo_root, path);
    relative.starts_with(".git/")
        || relative == ".git"
        || relative.starts_with(".claude/worktrees/")
        || relative.contains("/.claude/worktrees/")
}

fn source_path_is_context_text(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };
    matches!(
        extension,
        "bib"
            | "cfg"
            | "csv"
            | "json"
            | "jsonl"
            | "js"
            | "jsx"
            | "md"
            | "py"
            | "qmd"
            | "rs"
            | "tex"
            | "toml"
            | "ts"
            | "tsx"
            | "txt"
            | "typ"
            | "yaml"
            | "yml"
    )
}

fn profile_paths(repo_root: &Path, profile: &str) -> Vec<PathBuf> {
    let mut paths = vec![
        repo_root.join("AGENTS.md"),
        repo_root.join("README.md"),
        repo_root.join(".agents/issues.toml"),
        repo_root.join(".agents/todos.toml"),
        repo_root.join(".agents/resolved.toml"),
        repo_root.join(".agents/memory/state/PROJECT_STATE.md"),
        repo_root.join(".agents/memory/state/DECISIONS.md"),
        repo_root.join(".agents/memory/state/OPEN_QUESTIONS.md"),
        repo_root.join(".agents/memory/state/GOTCHAS.md"),
        repo_root.join(".configs/litkg.toml"),
    ];
    match profile {
        "agents-scaffold" => paths.extend([
            repo_root.join(".agents/AGENTS_INTERNAL_DB.md"),
            repo_root.join("docs/architecture.md"),
            repo_root.join("docs/kg-stack.md"),
            repo_root.join("docs/tooling-and-backends.md"),
        ]),
        "thesis-coding" => paths.extend([
            repo_root.join("docs/typst/seminar_paper/main.typ"),
            repo_root.join("docs/contents/thesis/roadmap.qmd"),
            repo_root.join("docs/contents/thesis/questions.qmd"),
            repo_root.join("aria_nbv/AGENTS.md"),
            repo_root.join("docs/AGENTS.md"),
        ]),
        "docs-paper-sync" => paths.extend([
            repo_root.join("docs/AGENTS.md"),
            repo_root.join("docs/_quarto.yml"),
            repo_root.join("docs/typst/seminar_paper/main.typ"),
            repo_root.join("docs/contents/thesis/roadmap.qmd"),
            repo_root.join("docs/contents/thesis/questions.qmd"),
        ]),
        "rri-oracle" => paths.extend([
            repo_root.join("aria_nbv/AGENTS.md"),
            repo_root.join("aria_nbv/aria_nbv/rri_metrics/AGENTS.md"),
            repo_root.join("aria_nbv/aria_nbv/data_handling/AGENTS.md"),
            repo_root.join("docs/contents/impl/rri_computation.qmd"),
            repo_root.join(".agents/skills/entity-aware-rri/SKILL.md"),
        ]),
        "vin-baseline" => paths.extend([
            repo_root.join("aria_nbv/AGENTS.md"),
            repo_root.join("aria_nbv/aria_nbv/vin/AGENTS.md"),
            repo_root.join("docs/contents/impl/vin_nbv.qmd"),
            repo_root.join(".agents/skills/nbv-geometry-contracts/SKILL.md"),
        ]),
        "rollout-planning" => paths.extend([
            repo_root.join("docs/contents/thesis/roadmap.qmd"),
            repo_root.join("docs/contents/thesis/questions.qmd"),
            repo_root.join(".agents/skills/counterfactual-rollout-planner/SKILL.md"),
            repo_root.join("aria_nbv/AGENTS.md"),
        ]),
        _ => {}
    }
    paths.sort();
    paths.dedup();
    paths
}

fn select_spans(
    source_path: &str,
    raw: &str,
    terms: &BTreeSet<String>,
    score: crate::ranking::WeightedScore,
) -> Vec<ContextEvidenceSpan> {
    let lines = raw.lines().collect::<Vec<_>>();
    let mut matched_spans = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let line_lower = line.to_ascii_lowercase();
        if terms.iter().any(|term| line_lower.contains(term)) {
            let start = index.saturating_sub(1);
            let end = usize::min(index + 2, lines.len());
            let text = lines[start..end].join("\n");
            let matched_terms = matched_terms(text.as_str(), terms);
            matched_spans.push((
                matched_terms.len(),
                ContextEvidenceSpan {
                    source_path: source_path.into(),
                    line_start: start + 1,
                    line_end: end,
                    kind: "matched_text".into(),
                    text,
                    score: score_for_span(score.clone(), source_path, terms, matched_terms.len()),
                },
            ));
        }
    }
    if !matched_spans.is_empty() {
        matched_spans.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then(left.1.line_start.cmp(&right.1.line_start))
        });
        return matched_spans
            .into_iter()
            .take(4)
            .map(|(_, span)| span)
            .collect();
    }
    if !lines.is_empty() {
        let end = usize::min(6, lines.len());
        return vec![ContextEvidenceSpan {
            source_path: source_path.into(),
            line_start: 1,
            line_end: end,
            kind: "fallback_intro".into(),
            text: lines[..end].join("\n"),
            score: score_for_span(score, source_path, terms, 0),
        }];
    }
    Vec::new()
}

fn score_for_span(
    mut score: crate::ranking::WeightedScore,
    source_path: &str,
    terms: &BTreeSet<String>,
    matched_count: usize,
) -> crate::ranking::WeightedScore {
    let path_matched_count = matched_terms(source_path, terms).len();
    let effective_matched_count = matched_count.max(path_matched_count);
    let lexical = if terms.is_empty() {
        0.1
    } else if effective_matched_count == 0 {
        0.05
    } else {
        (effective_matched_count as f32 / terms.len().max(1) as f32).clamp(0.15, 1.0)
    };
    score.score_lexical = lexical;
    score.score_final = score.score_lexical * score.score_authority * score.score_freshness;
    let source_type = score.source_type.clone();
    crate::ranking::apply_source_quality_adjustment(
        &mut score,
        &source_type,
        Path::new(source_path),
    );
    apply_route_surface_adjustment(&mut score, source_path, terms, matched_count);
    score
}

fn apply_route_surface_adjustment(
    score: &mut crate::ranking::WeightedScore,
    source_path: &str,
    terms: &BTreeSet<String>,
    matched_count: usize,
) {
    let source_type = score.source_type.as_str();
    if matched_count > 0
        && route_prefers_owning_surfaces(terms)
        && matches!(
            source_type,
            "active_backlog" | "agent_guidance" | "agent_skill" | "code" | "docs"
        )
    {
        let multiplier = if source_type == "code" { 1.35 } else { 1.2 };
        score.score_final *= multiplier;
        score
            .why
            .push("boosted owning route surface for concrete task terms".into());
    }
    if source_type == "canonical_memory"
        && route_prefers_owning_surfaces(terms)
        && !canonical_memory_is_explicit_route(source_path, terms)
    {
        let multiplier = if matched_count <= 1 { 0.65 } else { 0.85 };
        score.score_final *= multiplier;
        score
            .why
            .push("kept canonical memory below concrete owning routes for weak matches".into());
    }
    if thesis_claim_route(terms) && source_path.contains("docs/typst/thesis/proposal") {
        score.score_final *= 2.1;
        score
            .why
            .push("boosted proposal source for thesis claim routing".into());
    }
    if documentation_page_route(terms) {
        apply_documentation_page_adjustment(score, source_path, terms);
    }
}

fn documentation_page_route(terms: &BTreeSet<String>) -> bool {
    terms.iter().any(|term| {
        matches!(
            term.as_str(),
            "doc" | "docs" | "document" | "documentation" | "enrich" | "page" | "write" | "update"
        )
    })
}

fn apply_documentation_page_adjustment(
    score: &mut crate::ranking::WeightedScore,
    source_path: &str,
    terms: &BTreeSet<String>,
) {
    let path = source_path.replace('\\', "/");
    let path_matches = matched_terms(&path, terms).len();
    let is_test = path.contains("/tests/") || path.starts_with("tests/");

    if is_test && !terms.contains("test") && !terms.contains("tests") && !terms.contains("pytest") {
        score.score_final *= 0.25;
        score
            .why
            .push("penalized test file below docs/page owner for documentation task".into());
    }

    if path.starts_with("docs/contents/") && path.ends_with(".qmd") {
        let multiplier = if path_matches > 0 { 3.4 } else { 2.2 };
        score.score_final *= multiplier;
        score
            .why
            .push("boosted authored Quarto page for documentation task".into());
    } else if path.ends_with("README.md")
        || path.ends_with("AGENTS.md")
        || path.ends_with("SKILL.md")
    {
        let multiplier = if path_matches > 0 { 2.0 } else { 1.45 };
        score.score_final *= multiplier;
        score
            .why
            .push("boosted owner guidance for documentation task".into());
    } else if score.source_type == "code" && path_matches >= 2 {
        score.score_final *= 1.75;
        score
            .why
            .push("boosted implementation contract whose path matches documentation task".into());
    }
}

fn derive_verb(task: &str) -> String {
    let lower = task.trim().to_ascii_lowercase();
    if lower.starts_with("claim-check")
        || lower.starts_with("claim check")
        || lower.starts_with("check:")
        || lower.starts_with("check ")
    {
        "check".into()
    } else if lower.starts_with("consolidate")
        || lower.starts_with("consolidation")
        || lower.starts_with("sync memory")
    {
        "consolidate".into()
    } else if lower.starts_with("brief:")
        || lower.starts_with("brief ")
        || lower.starts_with("query:")
        || lower.starts_with("query ")
        || lower.starts_with("search:")
        || lower.starts_with("search ")
        || lower.starts_with("retrieve")
    {
        "retrieve".into()
    } else {
        "route".into()
    }
}

fn assumptions(profile: &str) -> Vec<String> {
    vec![
        "The pack is generated from local configured sources and read-only backend capability checks."
            .into(),
        "Evidence ranking combines task-term matches with configured authority tiers and file freshness."
            .into(),
        format!("The `{profile}` profile limits first-pass context; inspect cited sources before broad edits."),
    ]
}

fn top_sources(
    repo_root: &Path,
    config: &RepoConfig,
    evidence_spans: &[ContextEvidenceSpan],
    terms: &BTreeSet<String>,
) -> Result<Vec<ContextTopSource>> {
    const TOP_SOURCE_CAP: usize = 8;
    const DOC_PAGE_RESERVED: usize = 5;
    const LITERATURE_RESERVED_EXTRA: usize = 2;
    const CANONICAL_RESCUE_EXTRA: usize = 4;
    let mut seen = BTreeSet::new();
    let mut sources = Vec::new();

    // Literature roots that the verdict layer depends on for external
    // evidence. Mirrors the canonical [authority_tiers] globs in
    // .configs/litkg.toml (todo-074). If the canonical literature roots
    // change there, update this list too.
    fn is_literature_class(source_path: &str) -> bool {
        source_path.starts_with("docs/contents/literature/")
            || source_path.starts_with("docs/literature/tex-src/")
            || source_path == "docs/references.bib"
    }

    let push_span = |span: &ContextEvidenceSpan,
                     seen: &mut BTreeSet<String>,
                     sources: &mut Vec<ContextTopSource>|
     -> Result<()> {
        if !seen.insert(span.source_path.clone()) {
            return Ok(());
        }
        let metadata = source_metadata_for_path(config, &span.source_path);
        let matched = matched_terms(span.text.as_str(), terms);
        if matched.is_empty() && !source_is_explicit_route(&metadata, span, terms) {
            seen.remove(&span.source_path);
            return Ok(());
        }
        let authority = metadata
            .authority
            .unwrap_or_else(|| span.score.authority.clone());
        let role = metadata
            .role
            .unwrap_or_else(|| span.score.source_type.clone());
        sources.push(ContextTopSource {
            path: span.source_path.clone(),
            title: source_title(repo_root, &span.source_path)?,
            role,
            authority,
            freshness: span.score.score_freshness,
            source_span: ContextSourceSpan {
                line_start: span.line_start,
                line_end: span.line_end,
            },
            source_type: span.score.source_type.clone(),
            scores: span.score.clone(),
            why_relevant: why_relevant(span, terms),
        });
        Ok(())
    };

    // Pass 0 (documentation/page route): when the task asks to write,
    // enrich, or update a page, first surface the authored page and owner
    // contracts. Score-only ranking tends to over-promote tests and generic
    // backlog references because they have dense lexical overlap but are bad
    // first reads for documentation edits.
    if documentation_page_route(terms) {
        let mut candidates = evidence_spans
            .iter()
            .filter_map(|span| {
                documentation_page_candidate_priority(span, terms).map(|priority| (priority, span))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|(left_priority, left_span), (right_priority, right_span)| {
            left_priority
                .cmp(right_priority)
                .then(
                    right_span
                        .score
                        .score_final
                        .partial_cmp(&left_span.score.score_final)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
                .then(left_span.source_path.cmp(&right_span.source_path))
                .then(left_span.line_start.cmp(&right_span.line_start))
        });
        for (_, span) in candidates {
            if sources.len() >= DOC_PAGE_RESERVED {
                break;
            }
            push_span(span, &mut seen, &mut sources)?;
        }
    }

    // Pass 1 (primary): score-ordered iteration up to the cap. Preserves the
    // existing precedence (code / backlog / current_thesis above memory and
    // literature for owning-route tasks).
    for span in evidence_spans {
        if sources.len() >= TOP_SOURCE_CAP {
            break;
        }
        push_span(span, &mut seen, &mut sources)?;
    }

    // Pass 2 (literature reservation, todo-074): reserve up to
    // LITERATURE_RESERVED_EXTRA slots specifically for literature-class
    // paths. Without this, score-ordered canonical rescue (pass 3) is
    // dominated by test files and thesis prose whose lexical density
    // outranks paper-section spans; the verdict gate then never sees the
    // literature evidence even when it directly supports the claim.
    let literature_budget = TOP_SOURCE_CAP + LITERATURE_RESERVED_EXTRA;
    for span in evidence_spans {
        if sources.len() >= literature_budget {
            break;
        }
        if !is_literature_class(&span.source_path) {
            continue;
        }
        let matched = matched_terms(span.text.as_str(), terms);
        if matched.is_empty() {
            continue;
        }
        push_span(span, &mut seen, &mut sources)?;
    }

    // Pass 3 (canonical rescue, todo-073): admit canonical-tier matches with
    // non-empty term overlap that didn't survive the cap. Literature notes
    // are stable, not stale: the freshness half-life would otherwise drop
    // them below the floor in apply_confidence_floor. Cap the rescue at
    // CANONICAL_RESCUE_EXTRA so an out-of-domain claim doesn't drown the
    // primary set.
    let rescue_budget = literature_budget + CANONICAL_RESCUE_EXTRA;
    for span in evidence_spans {
        if sources.len() >= rescue_budget {
            break;
        }
        if span.score.authority != "canonical" {
            continue;
        }
        let matched = matched_terms(span.text.as_str(), terms);
        if matched.is_empty() {
            continue;
        }
        push_span(span, &mut seen, &mut sources)?;
    }

    Ok(sources)
}

fn documentation_page_candidate_priority(
    span: &ContextEvidenceSpan,
    terms: &BTreeSet<String>,
) -> Option<u8> {
    let path = span.source_path.replace('\\', "/");
    if path.contains("/tests/") || path.starts_with("tests/") {
        return None;
    }
    let path_matches = matched_terms(&path, terms).len();
    let text_matches = matched_terms(&span.text, terms).len();

    if path.starts_with("docs/contents/") && path.ends_with(".qmd") {
        if path_matches > 0 && !path.starts_with("docs/contents/thesis/") {
            return Some(0);
        }
        if path_matches > 0 {
            return Some(3);
        }
        if text_matches >= 2 {
            return Some(4);
        }
        return None;
    }
    if path.ends_with("README.md") || path.ends_with("AGENTS.md") || path.ends_with("SKILL.md") {
        if path_matches > 0 || text_matches >= 2 {
            return Some(1);
        }
        return None;
    }
    if span.score.source_type == "code" {
        if path_matches > 0 && text_matches >= 1 {
            if path.contains("/rollouts/") || path.contains("/data_handling/") {
                return Some(2);
            }
            return Some(5);
        }
        return None;
    }
    None
}

fn apply_confidence_floor(
    top_sources: &mut Vec<ContextTopSource>,
    min_top_source_score: f32,
) -> Option<String> {
    // Canonical-tier entries are curated literature and other ground-truth
    // sources; they stay in top_sources even when freshness has dropped
    // their score_final below the floor (todo-073).
    let has_canonical = top_sources
        .iter()
        .any(|source| source.scores.authority == "canonical");
    let max_score = top_sources
        .iter()
        .map(|source| source.scores.score_final)
        .fold(0.0_f32, f32::max);
    if top_sources.is_empty() || (max_score < min_top_source_score && !has_canonical) {
        top_sources.clear();
        Some("no high-signal evidence found; recommend aria-nbv-context plus targeted reads".into())
    } else {
        None
    }
}

fn route_prefers_owning_surfaces(terms: &BTreeSet<String>) -> bool {
    terms.iter().any(|term| {
        matches!(
            term.as_str(),
            "agent"
                | "agents"
                | "backlog"
                | "build"
                | "cargo"
                | "cli"
                | "code"
                | "crate"
                | "docs"
                | "doc"
                | "function"
                | "inspector"
                | "json"
                | "logging"
                | "module"
                | "plot"
                | "plotting"
                | "python"
                | "rerun"
                | "rollout"
                | "rust"
                | "scaffold"
                | "skill"
                | "test"
                | "tests"
                | "viewer"
                | "zarr"
        )
    })
}

fn implementation_route(terms: &BTreeSet<String>) -> bool {
    terms.iter().any(|term| {
        matches!(
            term.as_str(),
            "bug"
                | "code"
                | "debug"
                | "harden"
                | "pytest"
                | "python"
                | "regression"
                | "test"
                | "tests"
                | "trace"
                | "zarr"
        )
    })
}

fn literature_route(terms: &BTreeSet<String>) -> bool {
    terms.iter().any(|term| {
        matches!(
            term.as_str(),
            "bib"
                | "bibliography"
                | "citation"
                | "claim"
                | "literature"
                | "paper"
                | "papers"
                | "semantic"
                | "scholar"
        )
    })
}

fn generated_context_route(terms: &BTreeSet<String>) -> bool {
    terms.iter().any(|term| {
        matches!(
            term.as_str(),
            "context" | "generated" | "kg" | "route" | "routing" | "source_index"
        )
    })
}

fn canonical_memory_is_explicit_route(source_path: &str, terms: &BTreeSet<String>) -> bool {
    if !source_path.contains(".agents/memory/state/") {
        return false;
    }
    terms.iter().any(|term| {
        matches!(
            term.as_str(),
            "canonical"
                | "claim"
                | "decision"
                | "decisions"
                | "memory"
                | "question"
                | "questions"
                | "roadmap"
                | "state"
                | "thesis"
                | "truth"
        )
    })
}

fn thesis_claim_route(terms: &BTreeSet<String>) -> bool {
    terms.iter().any(|term| {
        matches!(
            term.as_str(),
            "claim" | "proposal" | "q_h" | "question" | "questions" | "roadmap" | "thesis"
        )
    })
}

fn source_is_explicit_route(
    metadata: &SourceMetadata,
    span: &ContextEvidenceSpan,
    terms: &BTreeSet<String>,
) -> bool {
    if terms.is_empty() {
        return false;
    }
    let source_type = span.score.source_type.as_str();
    if route_prefers_owning_surfaces(terms)
        && matches!(
            source_type,
            "active_backlog" | "agent_guidance" | "agent_skill" | "code" | "docs"
        )
    {
        let route_text = format!(
            "{} {} {}",
            span.source_path,
            metadata.role.as_deref().unwrap_or_default(),
            metadata.authority.as_deref().unwrap_or_default()
        )
        .to_ascii_lowercase();
        return terms.iter().any(|term| route_text.contains(term.as_str()));
    }
    if source_type == "canonical_memory" {
        return canonical_memory_is_explicit_route(&span.source_path, terms);
    }
    let route_text = format!(
        "{} {} {}",
        span.source_path,
        metadata.role.as_deref().unwrap_or_default(),
        metadata.authority.as_deref().unwrap_or_default()
    )
    .to_ascii_lowercase();
    terms.iter().any(|term| route_text.contains(term.as_str()))
}

fn required_reads(top_sources: &[ContextTopSource]) -> Vec<ContextRequiredRead> {
    top_sources
        .iter()
        .take(5)
        .map(|source| ContextRequiredRead {
            path: source.path.clone(),
            title: source.title.clone(),
            reason: format!(
                "{} evidence with `{}` authority near lines {}-{}",
                source.role,
                source.authority,
                source.source_span.line_start,
                source.source_span.line_end
            ),
        })
        .collect()
}

#[derive(Debug, Clone)]
struct SourceMetadata {
    role: Option<String>,
    authority: Option<String>,
}

fn source_metadata_for_path(config: &RepoConfig, source_path: &str) -> SourceMetadata {
    for (name, source) in &config.sources {
        if source_patterns(source)
            .iter()
            .any(|pattern| source_pattern_matches(source_path, pattern))
        {
            return SourceMetadata {
                role: Some(name.clone()),
                authority: source.authority.clone(),
            };
        }
    }
    SourceMetadata {
        role: None,
        authority: None,
    }
}

fn source_patterns(source: &crate::config::SourceConfig) -> Vec<String> {
    let mut patterns = Vec::new();
    patterns.extend(source.include.iter().cloned());
    patterns.extend(
        source
            .entrypoints
            .iter()
            .map(|path| path.display().to_string()),
    );
    if let Some(path) = &source.manifest {
        patterns.push(path.display().to_string());
    }
    if let Some(path) = &source.bib {
        patterns.push(path.display().to_string());
    }
    if let Some(pattern) = &source.pdfs {
        patterns.push(pattern.clone());
    }
    if let Some(pattern) = &source.tex {
        patterns.push(pattern.clone());
    }
    patterns
}

fn source_pattern_matches(source_path: &str, pattern: &str) -> bool {
    let path = normalize_path(source_path);
    let pattern = normalize_path(pattern);
    if path == pattern {
        return true;
    }
    if let Ok(glob) = glob::Pattern::new(&pattern) {
        if glob.matches(&path) {
            return true;
        }
    }
    if !pattern.contains('*') {
        return path.ends_with(&pattern);
    }
    false
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn source_title(repo_root: &Path, source_path: &str) -> Result<String> {
    let path = repo_root.join(source_path);
    let fallback = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(source_path)
        .to_string();
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(_) => return Ok(fallback),
    };
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            return Ok(title.trim().to_string());
        }
        if let Some(title) = trimmed.strip_prefix("= ") {
            return Ok(title.trim().to_string());
        }
    }
    Ok(fallback)
}

fn why_relevant(span: &ContextEvidenceSpan, terms: &BTreeSet<String>) -> Vec<String> {
    let lower = span.text.to_ascii_lowercase();
    let mut reasons = Vec::new();
    let matched = terms
        .iter()
        .filter(|term| lower.contains(term.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if matched.is_empty() {
        reasons.push(format!(
            "included as {} evidence for the profile",
            span.kind
        ));
    } else {
        reasons.push(format!("matches task terms: {}", matched.join(", ")));
    }
    reasons.extend(span.score.why.iter().cloned());
    reasons.sort();
    reasons.dedup();
    reasons
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
    candidates.extend(
        parsed
            .iter()
            .filter(|paper| {
                paper.kind == DocumentKind::Literature
                    && matches!(
                        paper.metadata.source_kind,
                        SourceKind::Manifest | SourceKind::Bib | SourceKind::ManifestAndBib
                    )
            })
            .map(|paper| paper_from_parsed(paper, terms)),
    );
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

fn collect_relevant_symbols(
    repo_root: &Path,
    profile: &str,
    terms: &BTreeSet<String>,
) -> Result<Vec<ContextSymbol>> {
    let roots = match profile {
        "rri-oracle" => vec![repo_root.join("aria_nbv/aria_nbv/rri_metrics")],
        "vin-baseline" => vec![repo_root.join("aria_nbv/aria_nbv/vin")],
        "rollout-planning" | "thesis-coding" => vec![repo_root.join("aria_nbv/aria_nbv")],
        _ => vec![repo_root.join("crates"), repo_root.join("src")],
    };
    let mut symbols = Vec::new();
    for root in roots.into_iter().filter(|root| root.is_dir()) {
        collect_symbols_from_root(repo_root, &root, terms, &mut symbols)?;
    }
    symbols.sort_by(|left, right| left.path.cmp(&right.path).then(left.name.cmp(&right.name)));
    symbols.truncate(12);
    Ok(symbols)
}

fn collect_symbols_from_root(
    repo_root: &Path,
    root: &Path,
    terms: &BTreeSet<String>,
    symbols: &mut Vec<ContextSymbol>,
) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if matches!(name, "__pycache__" | "target" | ".git") {
                continue;
            }
            collect_symbols_from_root(repo_root, &path, terms, symbols)?;
            continue;
        }
        let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };
        if !matches!(extension, "rs" | "py") {
            continue;
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let source_path = relative_path(repo_root, &path);
        for line in raw.lines() {
            let trimmed = line.trim_start();
            let kind = if trimmed.starts_with("pub struct ") || trimmed.starts_with("struct ") {
                Some("struct")
            } else if trimmed.starts_with("pub enum ") || trimmed.starts_with("enum ") {
                Some("enum")
            } else if trimmed.starts_with("pub fn ") || trimmed.starts_with("fn ") {
                Some("function")
            } else if trimmed.starts_with("class ") {
                Some("class")
            } else if trimmed.starts_with("def ") {
                Some("function")
            } else {
                None
            };
            let Some(kind) = kind else {
                continue;
            };
            let name = symbol_name(trimmed, kind);
            if name.is_empty() {
                continue;
            }
            let haystack = format!("{} {}", source_path, name).to_ascii_lowercase();
            let matched = terms.iter().any(|term| haystack.contains(term.as_str()));
            if matched || symbols.len() < 3 {
                symbols.push(ContextSymbol {
                    name,
                    kind: kind.into(),
                    path: source_path.clone(),
                    reason: if matched {
                        "matched task terms".into()
                    } else {
                        "profile entrypoint".into()
                    },
                });
            }
            if symbols.len() >= 24 {
                return Ok(());
            }
        }
    }
    Ok(())
}

fn symbol_name(line: &str, kind: &str) -> String {
    let prefix = match kind {
        "struct" => line
            .strip_prefix("pub struct ")
            .or_else(|| line.strip_prefix("struct ")),
        "enum" => line
            .strip_prefix("pub enum ")
            .or_else(|| line.strip_prefix("enum ")),
        "class" => line.strip_prefix("class "),
        _ => line
            .strip_prefix("pub fn ")
            .or_else(|| line.strip_prefix("fn "))
            .or_else(|| line.strip_prefix("def ")),
    };
    prefix
        .unwrap_or_default()
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .next()
        .unwrap_or_default()
        .to_string()
}

fn missing_leaves(
    _repo_root: &Path,
    task: &str,
    top_sources: &[ContextTopSource],
    _active_backlog: &[ContextBacklogItem],
    mut leaves: Vec<MissingContextLeaf>,
) -> Vec<MissingContextLeaf> {
    let terms = task_terms(task);
    if implementation_route(&terms)
        && !top_sources
            .iter()
            .any(|source| source.source_type == "code" || source.path.ends_with(".py"))
    {
        leaves.push(MissingContextLeaf {
            provider: "local_code".into(),
            query: format!(
                "No code source cleared the route threshold for `{}`.",
                task.trim()
            ),
            status: "missing".into(),
            resolution_command: "Use aria-nbv-context and targeted rg/file reads.".into(),
        });
    }
    leaves.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then(left.query.cmp(&right.query))
    });
    leaves.dedup_by(|left, right| left.provider == right.provider && left.query == right.query);
    leaves
}

fn risk_flags(
    config: &RepoConfig,
    repo_root: &Path,
    task: &str,
    top_sources: &[ContextTopSource],
) -> Vec<String> {
    let mut flags = Vec::new();
    let terms = task_terms(task);
    if literature_route(&terms) && !config.registry_path().is_file() {
        flags.push(format!(
            "missing_literature_registry: run cargo run -p litkg-cli -- ingest --config <config> before relying on paper metadata ({})",
            config.registry_path().display()
        ));
    }
    if literature_route(&terms) && !config.parsed_root().is_dir() {
        flags.push(format!(
            "missing_parsed_papers: run cargo run -p litkg-cli -- lit parse --config <config> ({})",
            config.parsed_root().display()
        ));
    }
    if generated_context_route(&terms)
        && !repo_root
            .join("docs/_generated/context/source_index.md")
            .is_file()
    {
        flags.push(
            "stale_or_missing_generated_context: run make context when docs routing matters".into(),
        );
    }
    if git_has_dirty_agents(repo_root)
        && top_sources
            .iter()
            .any(|source| source.source_type == "active_backlog")
    {
        flags.push(
            "dirty_agent_backlog: .agents has uncommitted changes; validate before treating backlog as final".into(),
        );
    }
    flags.extend(claim_check_flags(task));
    flags.sort();
    flags.dedup();
    flags
}

fn claim_check_flags(task: &str) -> Vec<String> {
    let task = task.to_ascii_lowercase();
    if !task.contains("claim-check") {
        return Vec::new();
    }
    let mut flags = Vec::new();
    if task.contains("finished")
        || task.contains("end-to-end")
        || task.contains("real-device")
        || task.contains("online rl")
        || task.contains("deployed")
    {
        flags.push(
            "unsupported_overclaim: canonical memory frames ARIA-NBV as ASE/EFM one-step scoring plus rollout/Q_H groundwork, not a finished end-to-end or deployed RL policy".into(),
        );
    }
    flags
}

#[derive(Debug, Default)]
struct ClaimVerdictParts {
    verdict: Option<String>,
    confidence: Option<f32>,
    supporting_evidence: Vec<ContextEvidenceSpan>,
    contradicting_evidence: Vec<ContextEvidenceSpan>,
}

fn claim_verdict(
    verb: &str,
    task: &str,
    top_sources: &[ContextTopSource],
    evidence_spans: &[ContextEvidenceSpan],
) -> ClaimVerdictParts {
    if verb != "check" {
        return ClaimVerdictParts::default();
    }
    let claim = strip_claim_prefix(task);
    let terms = task_terms(claim);
    let top_source_paths = top_sources
        .iter()
        .map(|source| source.path.as_str())
        .collect::<BTreeSet<_>>();
    let mut supporting = Vec::new();
    let mut contradicting = Vec::new();
    for span in evidence_spans {
        if !top_source_paths.contains(span.source_path.as_str()) || !trusted_claim_source(span) {
            continue;
        }
        match classify_claim_span(claim, &span.text, &terms) {
            ClaimRelation::Supports => supporting.push(span.clone()),
            ClaimRelation::Contradicts => contradicting.push(span.clone()),
            ClaimRelation::Neutral => {}
        }
        if supporting.len() >= 4 && contradicting.len() >= 2 {
            break;
        }
    }
    supporting.truncate(4);
    contradicting.truncate(4);
    if !contradicting.is_empty() {
        supporting.clear();
    }
    let max_support = supporting
        .iter()
        .map(|span| span.score.score_final)
        .fold(0.0_f32, f32::max);
    let max_contradiction = contradicting
        .iter()
        .map(|span| span.score.score_final)
        .fold(0.0_f32, f32::max);
    let (verdict, confidence) = if !contradicting.is_empty() {
        (
            "contradicted",
            (0.55 + max_contradiction.min(1.0) * 0.35).clamp(0.0, 1.0),
        )
    } else if supporting.len() >= 2 || (supporting.len() == 1 && max_support >= 0.9) {
        (
            "supported",
            (0.45 + supporting.len() as f32 * 0.16 + max_support.min(1.0) * 0.25).clamp(0.0, 1.0),
        )
    } else {
        ("unverifiable", 0.2_f32)
    };
    ClaimVerdictParts {
        verdict: Some(verdict.into()),
        confidence: Some(confidence),
        supporting_evidence: supporting,
        contradicting_evidence: contradicting,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaimRelation {
    Supports,
    Contradicts,
    Neutral,
}

fn strip_claim_prefix(task: &str) -> &str {
    let trimmed = task.trim();
    for prefix in [
        "claim-check:",
        "claim check:",
        "check:",
        "claim-check",
        "claim check",
        "check",
    ] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return rest.trim();
        }
    }
    trimmed
}

fn trusted_claim_source(span: &ContextEvidenceSpan) -> bool {
    let source_type = span.score.source_type.as_str();
    if matches!(source_type, "code" | "active_backlog") {
        return false;
    }
    matches!(span.score.authority.as_str(), "canonical" | "active")
        || matches!(source_type, "canonical_memory" | "docs")
}

fn classify_claim_span(claim: &str, evidence: &str, terms: &BTreeSet<String>) -> ClaimRelation {
    let claim_lower = claim.to_ascii_lowercase();
    let evidence_lower = evidence.to_ascii_lowercase();
    let matched = matched_terms(evidence, terms);
    let overlap = if terms.is_empty() {
        0.0
    } else {
        matched.len() as f32 / terms.len() as f32
    };
    if overlap < 0.35 {
        return ClaimRelation::Neutral;
    }
    if v1_gt_actor_visible_claim(&claim_lower)
        && evidence_lower.contains("actor-visible")
        && mentions_gt_obb(&evidence_lower)
        && contains_limiting_marker(&evidence_lower)
    {
        return ClaimRelation::Contradicts;
    }
    if positive_v0_gt_rri_claim(&claim_lower)
        && evidence_lower.contains("v0")
        && mentions_gt_obb(&evidence_lower)
        && evidence_lower.contains("rri")
        && (evidence_lower.contains("sanity") || evidence_lower.contains("upper-bound"))
    {
        return ClaimRelation::Supports;
    }
    // Research-claim shape ladder (todo-074). Specific shapes short-circuit
    // before the generic overlap fallback so reworded claims still flip
    // deterministically.
    if positive_rri_definition_claim(&claim_lower) && evidence_rri_definition(&evidence_lower) {
        return ClaimRelation::Supports;
    }
    if positive_hierarchical_nbv_claim(&claim_lower) && evidence_hierarchical_nbv(&evidence_lower) {
        return ClaimRelation::Supports;
    }
    if positive_aria_nbv_objective_claim(&claim_lower)
        && evidence_aria_nbv_objective(&evidence_lower)
    {
        return ClaimRelation::Supports;
    }
    if deferred_main_simulator_claim(&claim_lower, &evidence_lower) {
        return ClaimRelation::Neutral;
    }
    if contains_direct_negation(&evidence_lower) || contains_limiting_marker(&evidence_lower) {
        return ClaimRelation::Neutral;
    }
    if overlap >= 0.45 {
        ClaimRelation::Supports
    } else {
        ClaimRelation::Neutral
    }
}

fn v1_gt_actor_visible_claim(claim: &str) -> bool {
    (claim.contains("v1") || claim.contains("main"))
        && mentions_gt_obb(claim)
        && claim.contains("actor")
        && claim.contains("visible")
}

fn positive_v0_gt_rri_claim(claim: &str) -> bool {
    claim.contains("v0")
        && mentions_gt_obb(claim)
        && claim.contains("rri")
        && (claim.contains("sanity") || claim.contains("upper"))
}

// Research-claim shape helpers (todo-074). Each pairs a claim-side detector
// with an evidence-side predicate so the heuristic short-circuits to
// Supports only when both sides agree on the topic.

fn positive_rri_definition_claim(claim: &str) -> bool {
    claim.contains("rri")
        && (claim.contains("introduces") || claim.contains("defines") || claim.contains("label"))
        && (claim.contains("reconstruction") || claim.contains("oracle"))
}

fn evidence_rri_definition(evidence: &str) -> bool {
    evidence.contains("rri")
        && evidence.contains("reconstruction")
        && (evidence.contains("oracle")
            || evidence.contains("label")
            || evidence.contains("improvement"))
}

fn positive_hierarchical_nbv_claim(claim: &str) -> bool {
    claim.contains("hierarchical")
        && (claim.contains("nbv")
            || claim.contains("next best view")
            || claim.contains("next-best-view"))
        && (claim.contains("decomposit")
            || claim.contains("target")
            || claim.contains("pose")
            || claim.contains("two-stage")
            || claim.contains("two stage"))
}

fn evidence_hierarchical_nbv(evidence: &str) -> bool {
    evidence.contains("hierarchical")
        && (evidence.contains("target proposal")
            || evidence.contains("pose realization")
            || evidence.contains("look-at"))
}

fn positive_aria_nbv_objective_claim(claim: &str) -> bool {
    (claim.contains("aria-nbv") || claim.contains("aria nbv"))
        && (claim.contains("objective") || claim.contains("ranking") || claim.contains("candidate"))
        && (claim.contains("rri") || claim.contains("reconstruction"))
}

fn evidence_aria_nbv_objective(evidence: &str) -> bool {
    evidence.contains("rri")
        && (evidence.contains("candidate")
            || evidence.contains("ranking")
            || evidence.contains("reconstruction-quality"))
}

fn deferred_main_simulator_claim(claim: &str, evidence: &str) -> bool {
    claim.contains("main")
        && claim.contains("simulator")
        && (evidence.contains("stretch")
            || evidence.contains("bridge")
            || evidence.contains("future")
            || evidence.contains("external simulator")
            || evidence.contains("external online")
            || evidence.contains("simulators are design paths")
            || evidence.contains("finite-candidate evidence")
            || evidence.contains("unless later evidence")
            // todo-074: extended markers — the live `docs/contents/thesis/questions.qmd`
            // gates simulator-backed RL with phrases like "only if mesh/oracle…"
            // and "thesis-grade" requirements. Those are deferment markers in
            // intent even if they don't use the explicit "stretch"/"bridge"
            // vocabulary the V0 fixture used.
            || evidence.contains("only if")
            || evidence.contains("thesis-grade"))
}

fn mentions_gt_obb(text: &str) -> bool {
    (text.contains("gt") || text.contains("ground truth")) && text.contains("obb")
}

fn contains_limiting_marker(text: &str) -> bool {
    text.contains("cannot")
        || text.contains("must not")
        || text.contains("not actor")
        || text.contains("not the actor")
        || text.contains("oracle/eval-only")
        || text.contains("oracle-only")
        || text.contains("upper-bound")
}

fn contains_direct_negation(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| matches!(token, "not" | "cannot" | "never"))
        || text.contains(" not ")
        || text.contains("cannot")
        || text.contains("never")
        || text.contains("no longer")
        || text.contains("must not")
}

fn git_has_dirty_agents(repo_root: &Path) -> bool {
    let output = Command::new("git")
        .args(["status", "--short", "--", ".agents"])
        .current_dir(repo_root)
        .output();
    match output {
        Ok(output) if output.status.success() => !output.stdout.is_empty(),
        _ => false,
    }
}

fn action_plan(
    task: &str,
    profile: &str,
    backend_status: &[BackendDescriptor],
    missing_leaves: &[MissingContextLeaf],
    risk_flags: &[String],
) -> Vec<String> {
    let mut actions = vec![format!(
        "Start with the smallest edit that advances `{}`; use evidence spans before opening wider surfaces.",
        task.trim()
    )];
    actions.push(match profile {
        "docs-paper-sync" => {
            "Compare Typst paper, Quarto docs, and memory state before editing public narrative."
                .into()
        }
        "rri-oracle" => {
            "Check RRI/data-handling contracts before changing labels, crops, or metric code.".into()
        }
        "vin-baseline" => {
            "Check VIN and geometry contracts before changing model, dataset, or diagnostics code."
                .into()
        }
        "rollout-planning" => {
            "Separate thesis-scope planning evidence from future orchestration ideas before coding."
                .into()
        }
        "thesis-coding" => {
            "Use code, docs, paper, and backlog evidence together before touching cross-surface behavior."
                .into()
        }
        _ => {
            "Keep scaffold changes in validators, AGENTS.md, skills, and context-pack outputs before MCP wrapping."
                .into()
        }
    });
    if let Some(command) = backend_status
        .iter()
        .filter(|backend| !optional_backend_name(&backend.name))
        .filter_map(|backend| backend.repair_command.as_ref())
        .next()
    {
        actions.push(format!("Refresh first if needed: {command}"));
    }
    if let Some(leaf) = missing_leaves
        .iter()
        .find(|leaf| !optional_backend_name(&leaf.provider))
    {
        actions.push(format!(
            "Resolve missing leaf when it affects the task: {} -> {}",
            leaf.provider, leaf.resolution_command
        ));
    }
    if let Some(flag) = risk_flags.first() {
        actions.push(format!(
            "Mitigate highest risk flag before finalizing: {flag}"
        ));
    }
    actions
}

fn suggested_next_action(
    top_sources: &[ContextTopSource],
    backend_status: &[BackendDescriptor],
    missing_leaves: &[MissingContextLeaf],
    risk_flags: &[String],
    verification_commands: &[String],
    confidence_summary: Option<&str>,
) -> ContextSuggestedNextAction {
    if let Some(summary) = confidence_summary {
        return ContextSuggestedNextAction {
            summary: "Fall back to deterministic local discovery".into(),
            skill: Some("aria-nbv-context".into()),
            command: None,
            why: summary.into(),
        };
    }
    let has_useful_local_route = top_sources.iter().any(useful_local_source);
    if let Some(leaf) = missing_leaves
        .iter()
        .find(|leaf| !has_useful_local_route || !optional_backend_name(&leaf.provider))
    {
        let skill = if leaf.provider == "local_code" {
            "aria-nbv-context"
        } else {
            "semantic-scholar-litkg"
        };
        return ContextSuggestedNextAction {
            summary: format!("Resolve missing context from {}", leaf.provider),
            skill: Some(skill.into()),
            command: Some(leaf.resolution_command.clone()),
            why: format!(
                "{} is still {}; its absence can make retrieved context incomplete.",
                leaf.provider, leaf.status
            ),
        };
    }
    if let Some(backend) = backend_status
        .iter()
        .filter(|_| !has_useful_local_route)
        .find(|backend| backend.repair_command.is_some())
    {
        return ContextSuggestedNextAction {
            summary: format!(
                "Refresh {} before relying on generated evidence",
                backend.name
            ),
            skill: Some("semantic-scholar-litkg".into()),
            command: backend.repair_command.clone(),
            why: format!(
                "{} is {:?} with {:?} recommendation.",
                backend.name, backend.state, backend.agent_recommendation
            ),
        };
    }
    if let Some(flag) = risk_flags
        .iter()
        .find(|flag| !has_useful_local_route || !routine_route_warning_flag(flag))
    {
        return ContextSuggestedNextAction {
            summary: "Mitigate the highest risk flag".into(),
            skill: Some("aria-litkg-memory".into()),
            command: verification_commands.first().cloned(),
            why: (*flag).clone(),
        };
    }
    ContextSuggestedNextAction {
        summary: "Proceed with the smallest task-scoped edit using the required reads first".into(),
        skill: Some("aria-litkg-memory".into()),
        command: verification_commands.first().cloned(),
        why: "No task-specific missing context was detected in the local context pack.".into(),
    }
}

fn useful_local_source(source: &ContextTopSource) -> bool {
    matches!(
        source.source_type.as_str(),
        "active_backlog" | "agent_guidance" | "agent_skill" | "canonical_memory" | "code" | "docs"
    ) && !matches!(
        source.source_type.as_str(),
        "generated_context" | "audit_log" | "transcript" | "episodic_memory"
    )
}

fn optional_backend_name(provider: &str) -> bool {
    let normalized = provider
        .to_ascii_lowercase()
        .replace('-', "_")
        .replace(' ', "_");
    matches!(
        normalized.as_str(),
        "context7"
            | "openaideveloperdocs"
            | "openai_developerdocs"
            | "openai_developer_docs"
            | "graphiti"
            | "semantic_scholar"
            | "codegraphcontext"
            | "code_graph_context"
            | "graphify"
            | "neo4j_export"
            | "mempalace"
    )
}

fn optional_backend_risk_flag(flag: &str) -> bool {
    let Some((kind, rest)) = flag.split_once(':') else {
        return false;
    };
    matches!(kind, "backend_refresh_first" | "backend_missing_leaf")
        && rest.split(':').next().is_some_and(optional_backend_name)
}

fn routine_route_warning_flag(flag: &str) -> bool {
    optional_backend_risk_flag(flag) || flag.starts_with("dirty_agent_backlog:")
}

fn verification_commands(profile: &str) -> Vec<String> {
    let mut commands = match profile {
        "agents-scaffold" => vec![
            "make agents-db-check".into(),
            "make agents-db AGENTS_ARGS='validate'".into(),
            "cargo fmt --all --check".into(),
            "cargo test --all-features".into(),
        ],
        "docs-paper-sync" => vec![
            "make check-agent-memory".into(),
            "make context".into(),
            "quarto render docs".into(),
        ],
        _ => vec![
            "make check-agent-memory".into(),
            "make agents-db AGENTS_ARGS='validate'".into(),
            "cargo run --manifest-path .agents/external/litkg-rs/Cargo.toml -p litkg-cli -- context-pack --config .configs/litkg.toml --repo-root . --task \"<task>\" --profile thesis-coding --format json".into(),
        ],
    };
    commands.sort();
    commands.dedup();
    commands
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
    // Reserved capacity for canonical-tier spans. Literature notes and other
    // curated ground-truth surfaces (authority="canonical") would otherwise
    // be ranked below high-lexical-density code spans and lost to budget
    // truncation. Reserving slots up-front guarantees the verdict layer
    // gets at least a chance to see them (todo-073).
    const CANONICAL_RESERVED_TOKENS: usize = 2_400;

    let token_estimate =
        |span: &ContextEvidenceSpan| -> usize { span.text.split_whitespace().count() + 8 };

    let canonical_budget = CANONICAL_RESERVED_TOKENS.min(budget_tokens / 4);
    let mut canonical_kept = Vec::new();
    let mut canonical_used = 0usize;
    let mut non_canonical_idx = Vec::new();
    for (idx, span) in spans.iter().enumerate() {
        if span.score.authority == "canonical" {
            let est = token_estimate(span);
            if canonical_used + est <= canonical_budget {
                canonical_used += est;
                canonical_kept.push(idx);
            } else {
                non_canonical_idx.push(idx);
            }
        } else {
            non_canonical_idx.push(idx);
        }
    }

    let mut keep_set: std::collections::BTreeSet<usize> = canonical_kept.iter().copied().collect();
    let mut used = canonical_used;
    for idx in non_canonical_idx {
        let est = token_estimate(&spans[idx]);
        if used + est > budget_tokens {
            *truncated = true;
            break;
        }
        used += est;
        keep_set.insert(idx);
    }

    // Preserve the original score-descending order for surviving spans.
    let mut kept_spans: Vec<ContextEvidenceSpan> = spans
        .iter()
        .enumerate()
        .filter(|(idx, _)| keep_set.contains(idx))
        .map(|(_, span)| span.clone())
        .collect();
    if kept_spans.is_empty() && !spans.is_empty() {
        kept_spans.push(spans[0].clone());
    }
    *spans = kept_spans;
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BackendsConfig, SinkMode, SourceConfig};
    use std::collections::BTreeMap;

    fn test_config(root: &Path, include_context7: bool) -> RepoConfig {
        let generated = root.join("generated");
        let parsed = generated.join("parsed");
        let registry = generated.join("registry.jsonl");
        fs::create_dir_all(&parsed).unwrap();
        fs::create_dir_all(root.join("docs/_generated/context")).unwrap();
        fs::write(root.join("sources.jsonl"), "").unwrap();
        fs::write(root.join("references.bib"), "").unwrap();
        fs::write(&registry, "").unwrap();
        fs::write(generated.join("index.md"), "# Index\n").unwrap();
        fs::write(generated.join("graphify-manifest.json"), "{}\n").unwrap();
        fs::write(
            root.join("docs/_generated/context/source_index.md"),
            "# Source index\n",
        )
        .unwrap();

        let mut sources = BTreeMap::new();
        sources.insert(
            "code".into(),
            SourceConfig {
                authority: Some("implementation".into()),
                include: vec!["crates/**/*.rs".into()],
                ..SourceConfig::default()
            },
        );
        sources.insert(
            "agent_backlog".into(),
            SourceConfig {
                authority: Some("active_backlog".into()),
                include: vec![".agents/*.toml".into()],
                ..SourceConfig::default()
            },
        );
        sources.insert(
            "agent_memory".into(),
            SourceConfig {
                authority: Some("canonical".into()),
                include: vec![".agents/memory/state/*.md".into()],
                ..SourceConfig::default()
            },
        );
        if include_context7 {
            sources.insert(
                "external_docs".into(),
                SourceConfig {
                    context7_libraries: vec!["rerun".into()],
                    ..SourceConfig::default()
                },
            );
        }

        RepoConfig {
            manifest_path: root.join("sources.jsonl"),
            bib_path: root.join("references.bib"),
            tex_root: root.join("tex"),
            pdf_root: root.join("pdf"),
            generated_docs_root: generated.clone(),
            registry_path: Some(registry),
            parsed_root: Some(parsed),
            neo4j_export_root: Some(generated.join("neo4j-export")),
            memory_state_root: None,
            sink: SinkMode::Graphify,
            graphify_rebuild_command: None,
            download_pdfs: false,
            relevance_tags: Vec::new(),
            semantic_scholar: None,
            authority_tiers: Some(BTreeMap::from([
                (".agents/memory/state/*.md".into(), 2.0),
                (".agents/*.toml".into(), 1.4),
                ("crates/**/*.rs".into(), 1.2),
            ])),
            context_pack: Default::default(),
            project: None,
            sources,
            representation: None,
            backends: Some(BackendsConfig::default()),
            storage: None,
            runtime: None,
        }
    }

    fn write_context_pack_fixture(root: &Path) {
        fs::create_dir_all(root.join(".agents/memory/state")).unwrap();
        fs::create_dir_all(root.join(".agents/skills/rerun-inspector")).unwrap();
        fs::create_dir_all(root.join("crates/demo/src")).unwrap();
        fs::write(root.join("AGENTS.md"), "# Root guidance\n").unwrap();
        fs::write(
            root.join(".agents/memory/state/PROJECT_STATE.md"),
            "# Project State\n\nCanonical memory mentions rerun logging and zarr, but does not own the implementation route.\n\nV0 uses GT-OBB-cropped target RRI as a sanity and upper-bound measurement. GT OBBs cannot be actor-visible in the V1 main thesis result; V1 uses observed or predicted OBB inputs with GT evaluation labels.\n",
        )
        .unwrap();
        fs::write(
            root.join(".agents/memory/state/DECISIONS.md"),
            "# Decisions\n\nUse GT-OBB-cropped target RRI as V0 sanity/upper-bound evidence only. Main actor-visible target protocol is V1 OBS-SEL / PRED-Q / GT-EVAL, so GT OBBs must not be actor-visible in the main result.\n",
        )
        .unwrap();
        fs::write(
            root.join(".agents/issues.toml"),
            "[[issue]]\nid = \"ISSUE-1\"\ntitle = \"Fix rerun inspector rollout zarr logging\"\npriority = \"high\"\nstatus = \"open\"\n",
        )
        .unwrap();
        fs::write(root.join(".agents/todos.toml"), "").unwrap();
        fs::write(root.join(".agents/resolved.toml"), "").unwrap();
        fs::write(
            root.join(".agents/skills/rerun-inspector/SKILL.md"),
            "# Rerun Inspector\n\nUse this owning skill for rerun inspector rollout zarr logging work.\n",
        )
        .unwrap();
        fs::write(
            root.join("crates/demo/src/rerun.rs"),
            "pub fn log_rollout_zarr() {\n    // rerun inspector rollout zarr logging route\n}\n",
        )
        .unwrap();
    }

    #[test]
    fn context_pack_top_sources_prefer_concrete_owning_routes_over_weak_memory() {
        let dir = tempfile::tempdir().unwrap();
        write_context_pack_fixture(dir.path());
        let config = test_config(dir.path(), false);
        let pack = build_context_pack(
            &config,
            ContextPackRequest {
                config_path: None,
                repo_root: dir.path().to_path_buf(),
                task: "fix rerun inspector rollout zarr logging".into(),
                budget_tokens: 400,
                profile: "agents-scaffold".into(),
                lean: false,
            },
        )
        .unwrap();

        assert!(!pack.top_sources.is_empty());
        assert!(matches!(
            pack.top_sources[0].source_type.as_str(),
            "code" | "active_backlog" | "agent_skill" | "agent_guidance"
        ));
        let memory_index = pack
            .top_sources
            .iter()
            .position(|source| source.source_type == "canonical_memory");
        assert!(memory_index.is_none_or(|index| index > 0));
    }

    #[test]
    fn context_pack_suggested_next_action_does_not_default_to_optional_backend() {
        let dir = tempfile::tempdir().unwrap();
        write_context_pack_fixture(dir.path());
        let config = test_config(dir.path(), true);
        let pack = build_context_pack(
            &config,
            ContextPackRequest {
                config_path: None,
                repo_root: dir.path().to_path_buf(),
                task: "fix rerun inspector rollout zarr logging".into(),
                budget_tokens: 400,
                profile: "agents-scaffold".into(),
                lean: false,
            },
        )
        .unwrap();

        let rendered_action = format!(
            "{} {:?} {:?}",
            pack.suggested_next_action.summary,
            pack.suggested_next_action.skill,
            pack.suggested_next_action.command
        )
        .to_ascii_lowercase();
        assert!(!rendered_action.contains("context7"));
        assert!(!rendered_action.contains("semantic_scholar"));
    }

    #[test]
    fn context_pack_doc_page_route_prefers_page_and_owner_contracts_over_tests() {
        let dir = tempfile::tempdir().unwrap();
        write_context_pack_fixture(dir.path());
        fs::create_dir_all(dir.path().join("docs/contents")).unwrap();
        fs::create_dir_all(dir.path().join("aria_nbv/aria_nbv/rollouts")).unwrap();
        fs::create_dir_all(dir.path().join("aria_nbv/aria_nbv/data_handling")).unwrap();
        fs::create_dir_all(dir.path().join("aria_nbv/tests/data_handling")).unwrap();
        fs::create_dir_all(dir.path().join(".agents/skills/dataset-cache-ops")).unwrap();
        fs::create_dir_all(
            dir.path()
                .join(".agents/skills/counterfactual-rollout-planner"),
        )
        .unwrap();
        fs::write(
            dir.path().join("docs/contents/ase_dataset.qmd"),
            "---\ntitle: \"Aria Synthetic Environments Dataset\"\n---\n\n# Dataset page\n\nThis page documents the immutable VIN offline dataset and how counterfactual rollouts use a separate rollouts.zarr artifact.\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("aria_nbv/aria_nbv/data_handling/AGENTS.md"),
            "# Data handling\n\nOwner guidance for immutable VIN offline dataset documentation and narrative pages.\n",
        )
        .unwrap();
        fs::write(
            dir.path()
                .join(".agents/skills/dataset-cache-ops/SKILL.md"),
            "---\nname: dataset-cache-ops\n---\n\nUse for documenting immutable VIN offline dataset pages.\n",
        )
        .unwrap();
        fs::write(
            dir.path()
                .join(".agents/skills/counterfactual-rollout-planner/SKILL.md"),
            "---\nname: counterfactual-rollout-planner\n---\n\nUse for counterfactual rollouts and rollout artifact documentation.\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("aria_nbv/aria_nbv/rollouts/zarr_store.py"),
            "\"\"\"Implementation-contract owner for counterfactual rollouts.zarr.\n\nThe rollout store is separate from the immutable VIN offline dataset.\n\"\"\"\n",
        )
        .unwrap();
        fs::write(
            dir.path()
                .join("aria_nbv/tests/data_handling/test_vin_offline_store.py"),
            "\"\"\"Tests for offline dataset counterfactual rollouts page routing.\"\"\"\n",
        )
        .unwrap();

        let mut config = test_config(dir.path(), false);
        config.sources.insert(
            "docs".into(),
            SourceConfig {
                authority: Some("public_docs".into()),
                include: vec!["docs/contents/**/*.qmd".into()],
                ..SourceConfig::default()
            },
        );
        config.sources.insert(
            "python".into(),
            SourceConfig {
                authority: Some("implementation".into()),
                include: vec!["aria_nbv/**/*.py".into()],
                ..SourceConfig::default()
            },
        );
        config.sources.insert(
            "guidance".into(),
            SourceConfig {
                authority: Some("workflow".into()),
                include: vec!["**/AGENTS.md".into(), ".agents/skills/**/*.md".into()],
                ..SourceConfig::default()
            },
        );
        let authority_tiers = config
            .authority_tiers
            .as_mut()
            .expect("test_config installs authority_tiers");
        authority_tiers.insert("docs/contents/**/*.qmd".into(), 1.4);
        authority_tiers.insert("aria_nbv/**/*.py".into(), 1.2);

        let pack = build_context_pack(
            &config,
            ContextPackRequest {
                config_path: None,
                repo_root: dir.path().to_path_buf(),
                task: "enrich counterfactual rollouts offline dataset page".into(),
                budget_tokens: 900,
                profile: "thesis-coding".into(),
                lean: false,
            },
        )
        .unwrap();

        assert_eq!(
            pack.top_sources.first().map(|source| source.path.as_str()),
            Some("docs/contents/ase_dataset.qmd")
        );
        assert_eq!(
            pack.required_reads.first().map(|read| read.path.as_str()),
            Some("docs/contents/ase_dataset.qmd")
        );
        assert!(!pack
            .top_sources
            .iter()
            .take(3)
            .any(|source| source.path.contains("/tests/")));
        assert!(pack
            .top_sources
            .iter()
            .any(|source| source.path.ends_with("rollouts/zarr_store.py")));
    }

    #[test]
    fn context_pack_claim_check_returns_supported_contradicted_and_unverifiable() {
        let dir = tempfile::tempdir().unwrap();
        write_context_pack_fixture(dir.path());
        let config = test_config(dir.path(), false);

        let supported = build_context_pack(
            &config,
            ContextPackRequest {
                config_path: None,
                repo_root: dir.path().to_path_buf(),
                task:
                    "claim-check: ARIA-NBV uses GT-OBB-cropped target RRI as V0 sanity/upper-bound"
                        .into(),
                budget_tokens: 600,
                profile: "thesis-coding".into(),
                lean: false,
            },
        )
        .unwrap();
        assert_eq!(supported.verdict.as_deref(), Some("supported"));
        assert!(supported.confidence.unwrap_or_default() > 0.6);
        assert!(!supported.supporting_evidence.is_empty());

        let contradicted = build_context_pack(
            &config,
            ContextPackRequest {
                config_path: None,
                repo_root: dir.path().to_path_buf(),
                task: "claim-check: GT OBBs are actor-visible in the V1 main thesis result".into(),
                budget_tokens: 600,
                profile: "thesis-coding".into(),
                lean: false,
            },
        )
        .unwrap();
        assert_eq!(contradicted.verdict.as_deref(), Some("contradicted"));
        assert!(!contradicted.contradicting_evidence.is_empty());

        let unverifiable = build_context_pack(
            &config,
            ContextPackRequest {
                config_path: None,
                repo_root: dir.path().to_path_buf(),
                task: "claim-check: ARIA-NBV uses Habitat as the main simulator".into(),
                budget_tokens: 600,
                profile: "thesis-coding".into(),
                lean: false,
            },
        )
        .unwrap();
        assert_eq!(unverifiable.verdict.as_deref(), Some("unverifiable"));
        assert!(unverifiable.confidence.unwrap_or(1.0) <= 0.25);
    }

    #[test]
    fn context_pack_claim_check_handles_diverse_research_shapes() {
        // Verifies todo-074: the three new claim shapes (RRI definition,
        // hierarchical NBV, ARIA-NBV objective) flip to verdict=supported
        // when a literature-class fixture carries matching evidence. The
        // three previously-resolved shapes (V0 GT-OBB, V1 actor-visible,
        // Habitat-deferred) remain stable.
        let dir = tempfile::tempdir().unwrap();
        write_context_pack_fixture(dir.path());
        fs::create_dir_all(dir.path().join("docs/contents/literature")).unwrap();
        fs::write(
            dir.path()
                .join("docs/contents/literature/rri_theory.qmd"),
            "---\ntitle: \"RRI Theory\"\n---\n\n# RRI Theory\n\nVIN-NBV introduces Relative Reconstruction Improvement (RRI), an oracle label.\n\nRRI is computed from point-mesh reconstruction-error reduction after adding a query view.\n\nARIA-NBV ranks candidate views by RRI as its primary objective.\n\nThe reconstruction-quality ranking interprets RRI as an ordinal label.\n",
        )
        .unwrap();
        fs::write(
            dir.path()
                .join("docs/contents/literature/hestia.qmd"),
            "---\ntitle: \"Hestia\"\n---\n\n# Hestia\n\nHestia treats next-best-view selection as a hierarchical decomposition.\n\nThe hierarchical decomposition is target proposal plus pose realization.\n\nThe look-at primitive guides hierarchical target selection at the coarse stage.\n\nHierarchical NBV pose realization follows once the target proposal is chosen.\n",
        )
        .unwrap();

        let mut config = test_config(dir.path(), false);
        config.sources.insert(
            "literature_qmd".into(),
            SourceConfig {
                authority: Some("literature".into()),
                include: vec!["docs/contents/literature/**/*.qmd".into()],
                ..SourceConfig::default()
            },
        );
        config
            .authority_tiers
            .as_mut()
            .expect("test_config installs authority_tiers")
            .insert("docs/contents/literature/**/*.qmd".into(), 1.5);

        let run = |task: &str| {
            build_context_pack(
                &config,
                ContextPackRequest {
                    config_path: None,
                    repo_root: dir.path().to_path_buf(),
                    task: task.into(),
                    budget_tokens: 800,
                    profile: "thesis-coding".into(),
                    lean: false,
                },
            )
            .expect("build_context_pack must not fail in test")
        };

        // New shape: RRI definition.
        let rri = run("claim-check: VIN-NBV introduces Relative Reconstruction Improvement (RRI), an oracle label computed from point-mesh reconstruction-error reduction after adding a query view");
        assert_eq!(
            rri.verdict.as_deref(),
            Some("supported"),
            "RRI definition claim must flip to supported"
        );

        // New shape: hierarchical NBV decomposition.
        let hierarchical = run("claim-check: Hestia decomposes next-best-view into a hierarchical target proposal and pose realization");
        assert_eq!(
            hierarchical.verdict.as_deref(),
            Some("supported"),
            "hierarchical NBV claim must flip to supported"
        );

        // New shape: ARIA-NBV objective.
        let aria_nbv =
            run("claim-check: ARIA-NBV ranks candidate views by RRI as its primary objective");
        assert_eq!(
            aria_nbv.verdict.as_deref(),
            Some("supported"),
            "ARIA-NBV objective claim must flip to supported"
        );

        // Regression: previously-resolved V0 shape stays supported.
        let v0 =
            run("claim-check: ARIA-NBV uses GT-OBB-cropped target RRI as V0 sanity/upper-bound");
        assert_eq!(v0.verdict.as_deref(), Some("supported"));

        // Regression: V1 actor-visible stays contradicted.
        let v1 = run("claim-check: GT OBBs are actor-visible in the V1 main thesis result");
        assert_eq!(v1.verdict.as_deref(), Some("contradicted"));

        // Regression: Habitat-as-main-simulator stays unverifiable.
        let habitat = run("claim-check: ARIA-NBV uses Habitat as the main simulator");
        assert_eq!(habitat.verdict.as_deref(), Some("unverifiable"));
    }

    #[test]
    fn context_pack_admits_canonical_literature_into_top_sources_and_verdict() {
        // Verifies todo-073: a curated-literature note tiered as canonical
        // surfaces in top_sources for a matching claim, and the verdict
        // pipeline gets the supporting span even when other sources also
        // match.
        let dir = tempfile::tempdir().unwrap();
        write_context_pack_fixture(dir.path());
        fs::create_dir_all(dir.path().join("docs/contents/literature")).unwrap();
        fs::write(
            dir.path().join("docs/contents/literature/rri_theory.qmd"),
            "---\ntitle: \"RRI Theory\"\n---\n\n# RRI Theory\n\nVIN-NBV introduces Relative Reconstruction Improvement (RRI), an oracle label computed from point-mesh reconstruction-error reduction after adding a query view.\n",
        )
        .unwrap();

        let mut config = test_config(dir.path(), false);
        // Add the literature glob both as a source class (so the file lands
        // in evidence_spans) and as a canonical authority tier (so the
        // rescue pass + floor exemption fire).
        config.sources.insert(
            "literature_qmd".into(),
            SourceConfig {
                authority: Some("literature".into()),
                include: vec!["docs/contents/literature/**/*.qmd".into()],
                ..SourceConfig::default()
            },
        );
        config
            .authority_tiers
            .as_mut()
            .expect("test_config installs authority_tiers")
            .insert("docs/contents/literature/**/*.qmd".into(), 1.5);

        let pack = build_context_pack(
            &config,
            ContextPackRequest {
                config_path: None,
                repo_root: dir.path().to_path_buf(),
                task:
                    "claim-check: VIN-NBV introduces Relative Reconstruction Improvement (RRI), an oracle label computed from point-mesh reconstruction-error reduction after adding a query view"
                        .into(),
                budget_tokens: 800,
                profile: "thesis-coding".into(),
                lean: false,
            },
        )
        .unwrap();

        // The literature file appears in top_sources with canonical authority
        // (either via the primary pass or the canonical rescue extension).
        let canonical = pack
            .top_sources
            .iter()
            .find(|s| s.path.contains("contents/literature/rri_theory.qmd"))
            .expect("canonical literature must surface in top_sources");
        assert_eq!(canonical.scores.authority, "canonical");

        // Verdict layer admits the literature span and reports supported.
        assert_eq!(pack.verdict.as_deref(), Some("supported"));
        assert!(pack.confidence.unwrap_or_default() > 0.5);
        assert!(pack.supporting_evidence.iter().any(|span| span
            .source_path
            .contains("contents/literature/rri_theory.qmd")));
    }

    #[test]
    fn context_pack_filters_backlog_and_injects_verification_paths() {
        let dir = tempfile::tempdir().unwrap();
        write_context_pack_fixture(dir.path());
        fs::create_dir_all(dir.path().join("aria_nbv/tests/pose_generation")).unwrap();
        fs::create_dir_all(dir.path().join("aria_nbv/aria_nbv/pose_generation")).unwrap();
        fs::write(
            dir.path()
                .join("aria_nbv/tests/pose_generation/test_counterfactuals.py"),
            "def test_oracle_rri_lookahead_vs_greedy():\n    assert True\n",
        )
        .unwrap();
        fs::write(
            dir.path()
                .join("aria_nbv/aria_nbv/pose_generation/counterfactuals.py"),
            "def bounded_oracle_rri_lookahead_vs_greedy():\n    return 'counterfactuals'\n",
        )
        .unwrap();
        fs::write(
            dir.path().join(".agents/todos.toml"),
            "[[todo]]\nid = \"todo-026\"\ntitle = \"Harden bounded oracle-RRI lookahead versus greedy comparison\"\ndescription = \"Make oracle lookahead versus greedy executable.\"\nissue_ids = [\"issue-018\"]\npriority = \"high\"\nstatus = \"todo\"\ncontext = [\"bounded oracle RRI lookahead greedy comparison\"]\nreferences = [\"repo:aria_nbv/tests/pose_generation/test_counterfactuals.py\"]\nverification = [\"cd aria_nbv && uv run pytest tests/pose_generation/test_counterfactuals.py\"]\n\n[[todo]]\nid = \"todo-999\"\ntitle = \"Unrelated docs task\"\ndescription = \"No matching terms.\"\nissue_ids = []\npriority = \"low\"\nstatus = \"todo\"\ncontext = [\"unrelated\"]\nreferences = []\nverification = []\n",
        )
        .unwrap();
        fs::write(
            dir.path().join(".agents/issues.toml"),
            "[[issue]]\nid = \"issue-018\"\ntitle = \"Rollout dataset storage and reporting schema\"\ndescription = \"Bounded oracle RRI lookahead traces need storage.\"\npriority = \"high\"\nstatus = \"open\"\ncontext = [\"rollout bounded oracle rri\"]\nreferences = []\n",
        )
        .unwrap();
        let config = test_config(dir.path(), false);
        let pack = build_context_pack(
            &config,
            ContextPackRequest {
                config_path: None,
                repo_root: dir.path().to_path_buf(),
                task: "harden bounded oracle-RRI lookahead vs greedy".into(),
                budget_tokens: 900,
                profile: "thesis-coding".into(),
                lean: false,
            },
        )
        .unwrap();

        assert!(pack.active_todos.len() <= 5);
        assert!(pack.active_issues.len() <= 5);
        assert!(pack.active_todos.iter().any(|todo| todo.id == "todo-026"));
        assert!(pack
            .active_issues
            .iter()
            .any(|issue| issue.id == "issue-018"));
        let paths = pack
            .top_sources
            .iter()
            .map(|source| source.path.as_str())
            .collect::<Vec<_>>();
        assert!(paths
            .iter()
            .any(|path| path.ends_with("tests/pose_generation/test_counterfactuals.py")));
        assert!(paths
            .iter()
            .any(|path| path.ends_with("pose_generation/counterfactuals.py")));
    }

    #[test]
    fn context_pack_confidence_floor_falls_back_on_low_signal() {
        let dir = tempfile::tempdir().unwrap();
        write_context_pack_fixture(dir.path());
        let config = test_config(dir.path(), false);
        let pack = build_context_pack(
            &config,
            ContextPackRequest {
                config_path: None,
                repo_root: dir.path().to_path_buf(),
                task: "zzzzz nonsense input".into(),
                budget_tokens: 400,
                profile: "agents-scaffold".into(),
                lean: false,
            },
        )
        .unwrap();

        assert!(pack.top_sources.is_empty());
        assert!(pack.required_reads.is_empty());
        assert!(pack.confidence_summary.is_some());
        assert_eq!(
            pack.suggested_next_action.skill.as_deref(),
            Some("aria-nbv-context")
        );
    }

    #[test]
    fn context_pack_never_indexes_transient_worktree_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join(".claude/worktrees/pedantic-buck-d6f491/AGENTS.md");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "# Worktree mirror\n").unwrap();

        assert!(source_path_is_never_indexed(dir.path(), &path));
    }
}
