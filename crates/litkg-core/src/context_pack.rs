use crate::config::RepoConfig;
use crate::inspect::{compute_agent_conformance_report, AgentRecommendation, BackendDescriptor};
use crate::materialize::load_parsed_papers;
use crate::model::{PaperSourceRecord, ParsedPaper};
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
    pub profile: String,
    pub budget_tokens: usize,
    pub truncated: bool,
    pub task_summary: String,
    pub action_plan: Vec<String>,
    pub active_backlog: Vec<ContextBacklogItem>,
    pub active_issues: Vec<ContextBacklogItem>,
    pub active_todos: Vec<ContextBacklogItem>,
    pub evidence_spans: Vec<ContextEvidenceSpan>,
    pub relevant_symbols: Vec<ContextSymbol>,
    pub relevant_papers: Vec<ContextPaper>,
    pub missing_leaves: Vec<MissingContextLeaf>,
    pub missing_context_leaves: Vec<MissingContextLeaf>,
    pub risk_flags: Vec<String>,
    pub verification_commands: Vec<String>,
    pub backend_status: Vec<BackendDescriptor>,
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

    Ok(ContextPack {
        task_summary: format!("Context pack for: {}", request.task.trim()),
        task: request.task,
        profile: request.profile,
        budget_tokens: request.budget_tokens,
        truncated,
        action_plan,
        active_backlog,
        active_issues,
        active_todos,
        evidence_spans,
        relevant_symbols,
        relevant_papers,
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
                score: score.clone(),
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
            score,
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
        .filter_map(|backend| backend.repair_command.as_ref())
        .next()
    {
        actions.push(format!("Refresh first if needed: {command}"));
    }
    if let Some(leaf) = missing_leaves.first() {
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
