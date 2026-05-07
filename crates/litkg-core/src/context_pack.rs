use crate::config::RepoConfig;
use crate::inspect::{compute_agent_conformance_report, AgentRecommendation, BackendDescriptor};
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
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ContextPack {
    pub task: String,
    pub verb: String,
    pub profile: String,
    pub budget_tokens: usize,
    pub truncated: bool,
    pub task_summary: String,
    pub assumptions: Vec<String>,
    pub top_sources: Vec<ContextTopSource>,
    pub required_reads: Vec<ContextRequiredRead>,
    pub suggested_next_action: ContextSuggestedNextAction,
    pub action_plan: Vec<String>,
    pub active_backlog: Vec<ContextBacklogItem>,
    pub active_issues: Vec<ContextBacklogItem>,
    pub active_todos: Vec<ContextBacklogItem>,
    pub evidence_spans: Vec<ContextEvidenceSpan>,
    pub relevant_symbols: Vec<ContextSymbol>,
    pub relevant_papers: Vec<ContextPaper>,
    pub missing_context: Vec<MissingContextLeaf>,
    pub missing_leaves: Vec<MissingContextLeaf>,
    pub missing_context_leaves: Vec<MissingContextLeaf>,
    pub risk_flags: Vec<String>,
    pub verification_commands: Vec<String>,
    pub backend_status: Vec<BackendDescriptor>,
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
    let active_issues = load_active_issues(&repo_root)?;
    let active_todos = load_active_todos(&repo_root)?;
    let active_backlog = active_backlog(&active_issues, &active_todos);
    let mut evidence_spans =
        collect_evidence_spans(&repo_root, &request.profile, &task_terms, config)?;
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
    let missing_leaves = missing_leaves(config, &backend_status);
    let risk_flags = risk_flags(config, &repo_root, &request.task, &backend_status);
    let action_plan = action_plan(
        &request.task,
        &request.profile,
        &backend_status,
        &missing_leaves,
        &risk_flags,
    );
    let verification_commands = verification_commands(&request.profile);
    let mut truncated = false;

    truncate_spans_to_budget(&mut evidence_spans, request.budget_tokens, &mut truncated);
    let assumptions = assumptions(&request.profile);
    let top_sources = top_sources(&repo_root, config, &evidence_spans, &task_terms)?;
    let required_reads = required_reads(&top_sources);
    let suggested_next_action = suggested_next_action(
        &top_sources,
        &backend_status,
        &missing_leaves,
        &risk_flags,
        &verification_commands,
    );

    Ok(ContextPack {
        task_summary: format!("Context pack for: {}", request.task.trim()),
        verb: derive_verb(&request.task),
        task: request.task,
        profile: request.profile,
        budget_tokens: request.budget_tokens,
        truncated,
        assumptions,
        top_sources,
        required_reads,
        suggested_next_action,
        action_plan,
        active_backlog,
        active_issues,
        active_todos,
        evidence_spans,
        relevant_symbols,
        relevant_papers,
        missing_context: missing_leaves.clone(),
        missing_context_leaves: missing_leaves.clone(),
        missing_leaves,
        risk_flags,
        verification_commands,
        backend_status,
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
        .filter(|term| term.len() >= 3 && !crate::ranking::is_search_stopword(term))
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
    backlog.sort_by(|left, right| {
        priority_rank(&left.priority)
            .cmp(&priority_rank(&right.priority))
            .then(left.id.cmp(&right.id))
    });
    backlog
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
    for path in paths.into_iter().filter(|path| path.exists()) {
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
    spans.sort_by(|left, right| {
        // sort by final score descending, then source path
        right
            .score
            .score_final
            .partial_cmp(&left.score.score_final)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(left.source_path.cmp(&right.source_path))
            .then(left.line_start.cmp(&right.line_start))
    });
    Ok(spans)
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
                    {
                        paths.push(entry);
                    }
                }
            } else if pattern_path.is_file()
                && source_path_is_context_text(&pattern_path)
                && !source_path_is_excluded(repo_root, &pattern_path, &excludes)
            {
                paths.push(pattern_path);
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths.truncate(512);
    Ok(paths)
}

fn source_path_is_excluded(repo_root: &Path, path: &Path, excludes: &[String]) -> bool {
    let relative = relative_path(repo_root, path);
    excludes
        .iter()
        .any(|pattern| source_pattern_matches(&relative, pattern))
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
    let lexical = if terms.is_empty() {
        0.1
    } else if matched_count == 0 {
        0.05
    } else {
        (matched_count as f32 / terms.len().max(1) as f32).clamp(0.15, 1.0)
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
    let mut seen = BTreeSet::new();
    let mut sources = Vec::new();
    for span in evidence_spans {
        if !seen.insert(span.source_path.clone()) {
            continue;
        }
        let metadata = source_metadata_for_path(config, &span.source_path);
        let matched = matched_terms(span.text.as_str(), terms);
        if matched.is_empty() && !source_is_explicit_route(&metadata, span, terms) {
            continue;
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
        if sources.len() >= 8 {
            break;
        }
    }
    Ok(sources)
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
    config: &RepoConfig,
    backend_status: &[BackendDescriptor],
) -> Vec<MissingContextLeaf> {
    let mut leaves = vec![
        MissingContextLeaf {
            provider: "Context7".into(),
            query: "Resolve current library docs relevant to the task.".into(),
            status: "pending".into(),
            resolution_command:
                "Use Context7 MCP for configured libraries, then rerun litkg context-pack.".into(),
        },
        MissingContextLeaf {
            provider: "openaiDeveloperDocs".into(),
            query: "Resolve current OpenAI/Codex/MCP docs relevant to the task.".into(),
            status: "pending".into(),
            resolution_command:
                "Use openaiDeveloperDocs MCP when OpenAI/Codex/MCP behavior matters.".into(),
        },
    ];
    for backend in backend_status {
        if matches!(
            backend.agent_recommendation,
            AgentRecommendation::MissingLeaf | AgentRecommendation::RefreshFirst
        ) && backend.configured
        {
            leaves.push(MissingContextLeaf {
                provider: backend.name.clone(),
                query: format!(
                    "Backend is {:?}; resolve before treating generated context as current.",
                    backend.state
                ),
                status: "pending".into(),
                resolution_command: backend
                    .repair_command
                    .clone()
                    .unwrap_or_else(|| "inspect litkg capabilities --format json".into()),
            });
        }
    }
    let semantic = config.semantic_scholar_config();
    if semantic.enabled
        && std::env::var(&semantic.api_key_env)
            .ok()
            .is_none_or(|value| value.is_empty())
    {
        leaves.push(MissingContextLeaf {
            provider: "semantic_scholar".into(),
            query: "Semantic Scholar enrichment is configured but the API key is absent.".into(),
            status: "pending".into(),
            resolution_command: format!("export {}=...", semantic.api_key_env),
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
    backend_status: &[BackendDescriptor],
) -> Vec<String> {
    let mut flags = Vec::new();
    if !config.registry_path().is_file() {
        flags.push(format!(
            "missing_literature_registry: run cargo run -p litkg-cli -- ingest --config <config> before relying on paper metadata ({})",
            config.registry_path().display()
        ));
    }
    if !config.parsed_root().is_dir() {
        flags.push(format!(
            "missing_parsed_papers: run cargo run -p litkg-cli -- lit parse --config <config> ({})",
            config.parsed_root().display()
        ));
    }
    if !repo_root
        .join("docs/_generated/context/source_index.md")
        .is_file()
    {
        flags.push(
            "stale_or_missing_generated_context: run make context when docs routing matters".into(),
        );
    }
    if git_has_dirty_agents(repo_root) {
        flags.push(
            "dirty_agent_backlog: .agents has uncommitted changes; validate before treating backlog as final".into(),
        );
    }
    flags.extend(claim_check_flags(task));
    for backend in backend_status {
        match backend.agent_recommendation {
            AgentRecommendation::UseNow | AgentRecommendation::DoNotUse => {}
            AgentRecommendation::RefreshFirst => flags.push(format!(
                "backend_refresh_first:{}: {}",
                backend.name,
                backend
                    .repair_command
                    .as_deref()
                    .unwrap_or("inspect capabilities")
            )),
            AgentRecommendation::MissingLeaf => flags.push(format!(
                "backend_missing_leaf:{}: {}",
                backend.name,
                backend
                    .repair_command
                    .as_deref()
                    .unwrap_or("resolve missing source")
            )),
        }
    }
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
) -> ContextSuggestedNextAction {
    let has_useful_local_route = top_sources.iter().any(useful_local_source);
    if let Some(leaf) = missing_leaves
        .iter()
        .find(|leaf| !has_useful_local_route || !optional_backend_name(&leaf.provider))
    {
        return ContextSuggestedNextAction {
            summary: format!("Resolve missing context from {}", leaf.provider),
            skill: Some("semantic-scholar-litkg".into()),
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
            project: None,
            sources,
            representation: None,
            backends: Some(BackendsConfig::default()),
            storage: None,
        }
    }

    fn write_context_pack_fixture(root: &Path) {
        fs::create_dir_all(root.join(".agents/memory/state")).unwrap();
        fs::create_dir_all(root.join(".agents/skills/rerun-inspector")).unwrap();
        fs::create_dir_all(root.join("crates/demo/src")).unwrap();
        fs::write(root.join("AGENTS.md"), "# Root guidance\n").unwrap();
        fs::write(
            root.join(".agents/memory/state/PROJECT_STATE.md"),
            "# Project State\n\nCanonical memory mentions rerun logging and zarr, but does not own the implementation route.\n",
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
            },
        )
        .unwrap();

        assert!(pack
            .missing_context
            .iter()
            .any(|leaf| optional_backend_name(&leaf.provider)));
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
}
