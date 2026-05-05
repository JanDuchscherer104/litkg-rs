use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AgentBacklogKind {
    Issue,
    Todo,
}

impl AgentBacklogKind {
    pub fn node_label(self) -> &'static str {
        match self {
            Self::Issue => "AgentBacklogIssue",
            Self::Todo => "AgentBacklogTodo",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::Todo => "todo",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentBacklogRecord {
    pub id: String,
    pub kind: AgentBacklogKind,
    pub title: String,
    pub priority: String,
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub issue_ids: Vec<String>,
    #[serde(default)]
    pub context: Vec<String>,
    #[serde(default)]
    pub references: Vec<String>,
    pub source_path: String,
    pub line_start: usize,
    pub line_end: usize,
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

pub fn load_agent_backlog(repo_root: &Path) -> Result<Vec<AgentBacklogRecord>> {
    let mut records = Vec::new();
    records.extend(load_issues(repo_root)?);
    records.extend(load_todos(repo_root)?);
    records.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(records)
}

fn load_issues(repo_root: &Path) -> Result<Vec<AgentBacklogRecord>> {
    let path = repo_root.join(".agents/issues.toml");
    if !path.is_file() {
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
        .map(|issue| {
            let (line_start, line_end) = locate_record_lines(&raw, &issue.id);
            AgentBacklogRecord {
                id: issue.id,
                kind: AgentBacklogKind::Issue,
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
                source_path: repo_relative_path(&path, repo_root),
                line_start,
                line_end,
            }
        })
        .collect())
}

fn load_todos(repo_root: &Path) -> Result<Vec<AgentBacklogRecord>> {
    let path = repo_root.join(".agents/todos.toml");
    if !path.is_file() {
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
        .map(|todo| {
            let (line_start, line_end) = locate_record_lines(&raw, &todo.id);
            AgentBacklogRecord {
                id: todo.id,
                kind: AgentBacklogKind::Todo,
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
                source_path: repo_relative_path(&path, repo_root),
                line_start,
                line_end,
            }
        })
        .collect())
}

fn locate_record_lines(raw: &str, id: &str) -> (usize, usize) {
    let lines = raw.lines().collect::<Vec<_>>();
    let needle = format!("id = \"{id}\"");
    let Some(id_index) = lines.iter().position(|line| line.trim() == needle) else {
        return (1, lines.len().max(1));
    };
    let start = (0..=id_index)
        .rev()
        .find(|index| lines[*index].trim_start().starts_with("[["))
        .unwrap_or(id_index);
    let end = ((id_index + 1)..lines.len())
        .find(|index| lines[*index].trim_start().starts_with("[["))
        .unwrap_or(lines.len());
    (start + 1, end)
}

fn repo_relative_path(path: &Path, repo_root: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn config_repo_root(config_root: Option<&Path>, fallback: &Path) -> PathBuf {
    config_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| fallback.to_path_buf())
}
