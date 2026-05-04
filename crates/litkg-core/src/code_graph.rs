use crate::config::SourceConfig;
use crate::RepoConfig;
use anyhow::{Context, Result};
use glob::Pattern;
use rustpython_parser::{ast, Parse};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeGraph {
    pub repo_root: PathBuf,
    pub files: Vec<CodeFile>,
    pub modules: Vec<CodeModule>,
    pub symbols: Vec<CodeSymbol>,
    pub imports: Vec<CodeImport>,
    pub calls: Vec<CodeCall>,
    pub contains: Vec<CodeContainment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeFile {
    pub id: String,
    pub repo_path: String,
    pub module: String,
    pub line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeModule {
    pub id: String,
    pub name: String,
    pub file_id: String,
    pub repo_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSymbol {
    pub id: String,
    pub qualified_name: String,
    pub name: String,
    pub kind: CodeSymbolKind,
    pub module: String,
    pub file_id: String,
    pub repo_path: String,
    pub parent_id: Option<String>,
    pub line_start: usize,
    pub line_end: usize,
    pub signature: Option<String>,
    pub doc_summary: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeSymbolKind {
    Class,
    Function,
    Method,
    AsyncFunction,
    AsyncMethod,
}

impl CodeSymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Function => "function",
            Self::Method => "method",
            Self::AsyncFunction => "async_function",
            Self::AsyncMethod => "async_method",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeImport {
    pub source_id: String,
    pub source_file_id: String,
    pub imported: String,
    pub alias: Option<String>,
    pub line: usize,
    pub target_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeCall {
    pub source_id: String,
    pub source_file_id: String,
    pub target: String,
    pub line: usize,
    pub target_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CodeContainment {
    pub source_id: String,
    pub target_id: String,
    pub rel_type: String,
}

#[derive(Debug)]
struct ParsedPythonFile {
    repo_path: String,
    module: String,
    source: String,
    suite: ast::Suite,
    file_id: String,
    module_id: String,
    is_package_init: bool,
}

#[derive(Debug, Clone)]
struct Scope {
    source_id: String,
    file_id: String,
    module: String,
    class_qualified_name: Option<String>,
}

#[derive(Debug, Default)]
struct SymbolIndex {
    ids_by_qualified_name: BTreeMap<String, String>,
    ids_by_module: BTreeMap<String, String>,
    names_by_module: BTreeMap<String, BTreeMap<String, String>>,
    unique_qualified_name_by_symbol_name: BTreeMap<String, Option<String>>,
}

struct CodeSymbolInput<'a> {
    qualified_name: &'a str,
    name: &'a str,
    kind: CodeSymbolKind,
    file: &'a ParsedPythonFile,
    parent_id: Option<String>,
    range: rustpython_parser::text_size::TextRange,
    signature: Option<String>,
    doc_summary: Option<String>,
}

pub fn build_python_code_graph(config: &RepoConfig) -> Result<CodeGraph> {
    let repo_root = resolve_project_root(config)?;
    let Some(source) = config.sources.get("python") else {
        return Ok(empty_code_graph(repo_root));
    };
    if !source.symbols && source.edges.as_deref() != Some("codegraphcontext") {
        return Ok(empty_code_graph(repo_root));
    }

    let files = parse_python_files(&repo_root, source)?;
    let mut graph = CodeGraph {
        repo_root,
        files: Vec::new(),
        modules: Vec::new(),
        symbols: Vec::new(),
        imports: Vec::new(),
        calls: Vec::new(),
        contains: Vec::new(),
    };

    for file in &files {
        graph.files.push(CodeFile {
            id: file.file_id.clone(),
            repo_path: file.repo_path.clone(),
            module: file.module.clone(),
            line_count: line_count(&file.source),
        });
        graph.modules.push(CodeModule {
            id: file.module_id.clone(),
            name: file.module.clone(),
            file_id: file.file_id.clone(),
            repo_path: file.repo_path.clone(),
        });
        graph.contains.push(CodeContainment {
            source_id: file.file_id.clone(),
            target_id: file.module_id.clone(),
            rel_type: "REPRESENTS_MODULE".into(),
        });
        collect_symbol_definitions(file, &mut graph.symbols, &mut graph.contains);
    }

    let index = SymbolIndex::new(&graph.symbols, &graph.modules);
    for file in &files {
        let alias_map = collect_imports(file, &index, &mut graph.imports);
        let module_scope = Scope {
            source_id: file.module_id.clone(),
            file_id: file.file_id.clone(),
            module: file.module.clone(),
            class_qualified_name: None,
        };
        for stmt in &file.suite {
            collect_calls_from_stmt(
                stmt,
                file,
                &module_scope,
                &alias_map,
                &index,
                &mut graph.calls,
            );
        }
    }

    graph.files.sort_by(|left, right| left.id.cmp(&right.id));
    graph.modules.sort_by(|left, right| left.id.cmp(&right.id));
    graph
        .symbols
        .sort_by(|left, right| left.qualified_name.cmp(&right.qualified_name));
    graph.imports.sort_by(|left, right| {
        left.source_id
            .cmp(&right.source_id)
            .then_with(|| left.imported.cmp(&right.imported))
            .then_with(|| left.line.cmp(&right.line))
    });
    graph.calls.sort_by(|left, right| {
        left.source_id
            .cmp(&right.source_id)
            .then_with(|| left.target.cmp(&right.target))
            .then_with(|| left.line.cmp(&right.line))
    });
    graph.contains.sort();
    graph.imports.dedup();
    graph.calls.dedup();
    graph.contains.dedup();

    Ok(graph)
}

fn empty_code_graph(repo_root: PathBuf) -> CodeGraph {
    CodeGraph {
        repo_root,
        files: Vec::new(),
        modules: Vec::new(),
        symbols: Vec::new(),
        imports: Vec::new(),
        calls: Vec::new(),
        contains: Vec::new(),
    }
}

fn project_root(config: &RepoConfig) -> PathBuf {
    config
        .project
        .as_ref()
        .map(|project| project.root.clone())
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn resolve_project_root(config: &RepoConfig) -> Result<PathBuf> {
    let root = project_root(config);
    let absolute = if root.is_absolute() {
        root
    } else {
        std::env::current_dir()?.join(root)
    };
    absolute
        .canonicalize()
        .with_context(|| format!("Failed to resolve project root {}", absolute.display()))
}

fn parse_python_files(repo_root: &Path, source: &SourceConfig) -> Result<Vec<ParsedPythonFile>> {
    let exclude_patterns = compile_excludes(&source.exclude)?;
    let mut paths = BTreeSet::new();
    for pattern in &source.include {
        let full_pattern = repo_root.join(pattern);
        let full_pattern = full_pattern
            .to_str()
            .with_context(|| format!("invalid python source glob {}", full_pattern.display()))?;
        for entry in glob::glob(full_pattern)
            .with_context(|| format!("failed to expand python source glob {pattern}"))?
        {
            let path = entry.with_context(|| format!("failed to read path from glob {pattern}"))?;
            if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("py") {
                continue;
            }
            let rel = repo_relative_path(repo_root, &path)?;
            if is_excluded(&rel, &exclude_patterns) {
                continue;
            }
            paths.insert(path);
        }
    }

    let mut files = Vec::new();
    for path in paths {
        let source = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read Python source {}", path.display()))?;
        let suite = ast::Suite::parse(&source, path.to_string_lossy().as_ref())
            .with_context(|| format!("Failed to parse Python source {}", path.display()))?;
        let repo_path = repo_relative_path(repo_root, &path)?;
        let is_package_init =
            path.file_name().and_then(|name| name.to_str()) == Some("__init__.py");
        let module = module_name(repo_root, &path)?;
        let file_id = format!("code_file:{repo_path}");
        let module_id = format!("code_module:{module}");
        files.push(ParsedPythonFile {
            repo_path,
            module,
            source,
            suite,
            file_id,
            module_id,
            is_package_init,
        });
    }
    files.sort_by(|left, right| left.repo_path.cmp(&right.repo_path));
    Ok(files)
}

fn compile_excludes(patterns: &[String]) -> Result<Vec<Pattern>> {
    patterns
        .iter()
        .map(|pattern| {
            Pattern::new(pattern).with_context(|| format!("invalid exclude pattern {pattern}"))
        })
        .collect()
}

fn is_excluded(repo_path: &str, patterns: &[Pattern]) -> bool {
    patterns.iter().any(|pattern| pattern.matches(repo_path))
}

fn repo_relative_path(repo_root: &Path, path: &Path) -> Result<String> {
    let rel = path
        .strip_prefix(repo_root)
        .with_context(|| format!("{} is not under {}", path.display(), repo_root.display()))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

fn module_name(repo_root: &Path, path: &Path) -> Result<String> {
    let rel = path
        .strip_prefix(repo_root)
        .with_context(|| format!("{} is not under {}", path.display(), repo_root.display()))?;
    let components = rel
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let package_start = components
        .iter()
        .enumerate()
        .find_map(|(index, _)| {
            let package_dir = repo_root.join(components[..=index].iter().collect::<PathBuf>());
            package_dir.join("__init__.py").is_file().then_some(index)
        })
        .unwrap_or(0);
    let mut module_parts = components[package_start..].to_vec();
    if let Some(last) = module_parts.last_mut() {
        if let Some(stem) = last.strip_suffix(".py") {
            *last = stem.to_string();
        }
    }
    if module_parts.last().map(String::as_str) == Some("__init__") {
        module_parts.pop();
    }
    Ok(module_parts.join("."))
}

fn collect_symbol_definitions(
    file: &ParsedPythonFile,
    symbols: &mut Vec<CodeSymbol>,
    contains: &mut Vec<CodeContainment>,
) {
    let module_scope = file.module_id.clone();
    for stmt in &file.suite {
        collect_symbol_definition(stmt, file, &module_scope, None, symbols, contains);
    }
}

fn collect_symbol_definition(
    stmt: &ast::Stmt,
    file: &ParsedPythonFile,
    parent_id: &str,
    parent_qualified_name: Option<&str>,
    symbols: &mut Vec<CodeSymbol>,
    contains: &mut Vec<CodeContainment>,
) {
    match stmt {
        ast::Stmt::ClassDef(class_def) => {
            let qualified_name = qualified_child_name(
                parent_qualified_name.unwrap_or(file.module.as_str()),
                class_def.name.as_str(),
            );
            let symbol = code_symbol(CodeSymbolInput {
                qualified_name: &qualified_name,
                name: class_def.name.as_str(),
                kind: CodeSymbolKind::Class,
                file,
                parent_id: Some(parent_id.to_string()),
                range: class_def.range,
                signature: None,
                doc_summary: doc_summary(&class_def.body),
            });
            let symbol_id = symbol.id.clone();
            symbols.push(symbol);
            contains.push(CodeContainment {
                source_id: parent_id.to_string(),
                target_id: symbol_id.clone(),
                rel_type: "CONTAINS".into(),
            });
            contains.push(CodeContainment {
                source_id: file.file_id.clone(),
                target_id: symbol_id.clone(),
                rel_type: "DEFINES".into(),
            });
            for child in &class_def.body {
                collect_symbol_definition(
                    child,
                    file,
                    &symbol_id,
                    Some(qualified_name.as_str()),
                    symbols,
                    contains,
                );
            }
        }
        ast::Stmt::FunctionDef(function_def) => {
            let qualified_name = qualified_child_name(
                parent_qualified_name.unwrap_or(file.module.as_str()),
                function_def.name.as_str(),
            );
            let kind = if parent_qualified_name.is_some() {
                CodeSymbolKind::Method
            } else {
                CodeSymbolKind::Function
            };
            let symbol = code_symbol(CodeSymbolInput {
                qualified_name: &qualified_name,
                name: function_def.name.as_str(),
                kind,
                file,
                parent_id: Some(parent_id.to_string()),
                range: function_def.range,
                signature: Some(function_signature(&function_def.args)),
                doc_summary: doc_summary(&function_def.body),
            });
            let symbol_id = symbol.id.clone();
            symbols.push(symbol);
            contains.push(CodeContainment {
                source_id: parent_id.to_string(),
                target_id: symbol_id.clone(),
                rel_type: "CONTAINS".into(),
            });
            contains.push(CodeContainment {
                source_id: file.file_id.clone(),
                target_id: symbol_id.clone(),
                rel_type: "DEFINES".into(),
            });
        }
        ast::Stmt::AsyncFunctionDef(function_def) => {
            let qualified_name = qualified_child_name(
                parent_qualified_name.unwrap_or(file.module.as_str()),
                function_def.name.as_str(),
            );
            let kind = if parent_qualified_name.is_some() {
                CodeSymbolKind::AsyncMethod
            } else {
                CodeSymbolKind::AsyncFunction
            };
            let symbol = code_symbol(CodeSymbolInput {
                qualified_name: &qualified_name,
                name: function_def.name.as_str(),
                kind,
                file,
                parent_id: Some(parent_id.to_string()),
                range: function_def.range,
                signature: Some(function_signature(&function_def.args)),
                doc_summary: doc_summary(&function_def.body),
            });
            let symbol_id = symbol.id.clone();
            symbols.push(symbol);
            contains.push(CodeContainment {
                source_id: parent_id.to_string(),
                target_id: symbol_id.clone(),
                rel_type: "CONTAINS".into(),
            });
            contains.push(CodeContainment {
                source_id: file.file_id.clone(),
                target_id: symbol_id,
                rel_type: "DEFINES".into(),
            });
        }
        _ => {}
    }
}

fn code_symbol(input: CodeSymbolInput<'_>) -> CodeSymbol {
    CodeSymbol {
        id: format!("code_symbol:{}", input.qualified_name),
        qualified_name: input.qualified_name.to_string(),
        name: input.name.to_string(),
        kind: input.kind,
        module: input.file.module.clone(),
        file_id: input.file.file_id.clone(),
        repo_path: input.file.repo_path.clone(),
        parent_id: input.parent_id,
        line_start: offset_to_line(&input.file.source, input.range.start()),
        line_end: offset_to_line(&input.file.source, input.range.end()),
        signature: input.signature,
        doc_summary: input.doc_summary,
    }
}

fn qualified_child_name(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}.{child}")
    }
}

fn function_signature(args: &ast::Arguments) -> String {
    let mut parts = Vec::new();
    for arg in args.posonlyargs.iter().chain(args.args.iter()) {
        parts.push(arg.def.arg.to_string());
    }
    if let Some(vararg) = &args.vararg {
        parts.push(format!("*{}", vararg.arg));
    } else if !args.kwonlyargs.is_empty() {
        parts.push("*".into());
    }
    for arg in &args.kwonlyargs {
        parts.push(arg.def.arg.to_string());
    }
    if let Some(kwarg) = &args.kwarg {
        parts.push(format!("**{}", kwarg.arg));
    }
    format!("({})", parts.join(", "))
}

fn doc_summary(body: &[ast::Stmt]) -> Option<String> {
    let Some(ast::Stmt::Expr(expr_stmt)) = body.first() else {
        return None;
    };
    let ast::Expr::Constant(constant) = expr_stmt.value.as_ref() else {
        return None;
    };
    let ast::Constant::Str(text) = &constant.value else {
        return None;
    };
    text.trim()
        .split("\n\n")
        .next()
        .map(str::trim)
        .filter(|summary| !summary.is_empty())
        .map(ToOwned::to_owned)
}

impl SymbolIndex {
    fn new(symbols: &[CodeSymbol], modules: &[CodeModule]) -> Self {
        let mut index = Self::default();
        for module in modules {
            index
                .ids_by_module
                .insert(module.name.clone(), module.id.clone());
        }
        for symbol in symbols {
            index
                .ids_by_qualified_name
                .insert(symbol.qualified_name.clone(), symbol.id.clone());
            index
                .names_by_module
                .entry(symbol.module.clone())
                .or_default()
                .insert(symbol.name.clone(), symbol.qualified_name.clone());
            index
                .unique_qualified_name_by_symbol_name
                .entry(symbol.name.clone())
                .and_modify(|existing| {
                    if existing.as_deref() != Some(symbol.qualified_name.as_str()) {
                        *existing = None;
                    }
                })
                .or_insert_with(|| Some(symbol.qualified_name.clone()));
        }
        index
    }

    fn id_for(&self, qualified_name: &str) -> Option<String> {
        self.ids_by_qualified_name.get(qualified_name).cloned()
    }

    fn id_for_module(&self, module: &str) -> Option<String> {
        self.ids_by_module.get(module).cloned()
    }

    fn id_for_entity(&self, qualified_name: &str) -> Option<String> {
        self.id_for(qualified_name)
            .or_else(|| self.id_for_module(qualified_name))
    }

    fn unique_symbol_id_for_name(&self, name: &str) -> Option<String> {
        self.unique_qualified_name_by_symbol_name
            .get(name)
            .and_then(|qualified| qualified.as_deref())
            .and_then(|qualified| self.id_for(qualified))
    }

    fn resolve_import(&self, imported: &str) -> Option<String> {
        self.id_for_entity(imported).or_else(|| {
            imported
                .rsplit('.')
                .next()
                .and_then(|name| self.unique_symbol_id_for_name(name))
        })
    }

    fn has_symbol(&self, qualified_name: &str) -> bool {
        self.ids_by_qualified_name.contains_key(qualified_name)
    }

    fn module_symbol(&self, module: &str, name: &str) -> Option<String> {
        self.names_by_module
            .get(module)
            .and_then(|names| names.get(name))
            .cloned()
    }
}

fn collect_imports(
    file: &ParsedPythonFile,
    index: &SymbolIndex,
    imports: &mut Vec<CodeImport>,
) -> BTreeMap<String, String> {
    let mut alias_map = BTreeMap::new();
    let module_scope = file.module_id.clone();
    for stmt in &file.suite {
        collect_imports_from_stmt(stmt, file, &module_scope, index, &mut alias_map, imports);
    }
    alias_map
}

fn collect_imports_from_stmt(
    stmt: &ast::Stmt,
    file: &ParsedPythonFile,
    scope_id: &str,
    index: &SymbolIndex,
    alias_map: &mut BTreeMap<String, String>,
    imports: &mut Vec<CodeImport>,
) {
    match stmt {
        ast::Stmt::Import(import_stmt) => {
            for alias in &import_stmt.names {
                let imported = alias.name.to_string();
                let visible_name = alias
                    .asname
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| {
                        imported
                            .split('.')
                            .next()
                            .unwrap_or(imported.as_str())
                            .into()
                    });
                alias_map.insert(visible_name.clone(), imported.clone());
                imports.push(CodeImport {
                    source_id: scope_id.to_string(),
                    source_file_id: file.file_id.clone(),
                    imported: imported.clone(),
                    alias: (visible_name != imported).then_some(visible_name),
                    line: offset_to_line(&file.source, alias.range.start()),
                    target_id: index.resolve_import(imported.as_str()),
                });
            }
        }
        ast::Stmt::ImportFrom(import_stmt) => {
            let base_module = resolve_import_from_module(
                file.module.as_str(),
                file.is_package_init,
                import_stmt.module.as_ref().map(|item| item.as_str()),
                import_stmt.level.map(|level| level.to_usize()).unwrap_or(0),
            );
            for alias in &import_stmt.names {
                if alias.name.as_str() == "*" {
                    continue;
                }
                let imported = if base_module.is_empty() {
                    alias.name.to_string()
                } else {
                    format!("{base_module}.{}", alias.name)
                };
                let visible_name = alias
                    .asname
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| alias.name.to_string());
                alias_map.insert(visible_name.clone(), imported.clone());
                imports.push(CodeImport {
                    source_id: scope_id.to_string(),
                    source_file_id: file.file_id.clone(),
                    imported: imported.clone(),
                    alias: (visible_name != alias.name.as_str()).then_some(visible_name),
                    line: offset_to_line(&file.source, alias.range.start()),
                    target_id: index.resolve_import(imported.as_str()),
                });
            }
        }
        ast::Stmt::FunctionDef(function_def) => {
            let function_scope_id = index
                .module_symbol(&file.module, function_def.name.as_str())
                .and_then(|qualified| index.id_for(qualified.as_str()))
                .unwrap_or_else(|| scope_id.to_string());
            for child in &function_def.body {
                collect_imports_from_stmt(
                    child,
                    file,
                    &function_scope_id,
                    index,
                    alias_map,
                    imports,
                );
            }
        }
        ast::Stmt::AsyncFunctionDef(function_def) => {
            let function_scope_id = index
                .module_symbol(&file.module, function_def.name.as_str())
                .and_then(|qualified| index.id_for(qualified.as_str()))
                .unwrap_or_else(|| scope_id.to_string());
            for child in &function_def.body {
                collect_imports_from_stmt(
                    child,
                    file,
                    &function_scope_id,
                    index,
                    alias_map,
                    imports,
                );
            }
        }
        ast::Stmt::ClassDef(class_def) => {
            let qualified_name =
                qualified_child_name(file.module.as_str(), class_def.name.as_str());
            let class_scope_id = index
                .id_for(qualified_name.as_str())
                .unwrap_or_else(|| scope_id.to_string());
            for child in &class_def.body {
                collect_imports_from_stmt(child, file, &class_scope_id, index, alias_map, imports);
            }
        }
        _ => walk_child_statements_for_imports(stmt, file, scope_id, index, alias_map, imports),
    }
}

fn walk_child_statements_for_imports(
    stmt: &ast::Stmt,
    file: &ParsedPythonFile,
    scope_id: &str,
    index: &SymbolIndex,
    alias_map: &mut BTreeMap<String, String>,
    imports: &mut Vec<CodeImport>,
) {
    for block in stmt_child_blocks(stmt).into_iter().flatten() {
        for child in block {
            collect_imports_from_stmt(child, file, scope_id, index, alias_map, imports);
        }
    }
}

fn collect_calls_from_stmt(
    stmt: &ast::Stmt,
    file: &ParsedPythonFile,
    scope: &Scope,
    alias_map: &BTreeMap<String, String>,
    index: &SymbolIndex,
    calls: &mut Vec<CodeCall>,
) {
    match stmt {
        ast::Stmt::ClassDef(class_def) => {
            let class_qualified_name =
                qualified_child_name(file.module.as_str(), class_def.name.as_str());
            let class_scope = Scope {
                source_id: index
                    .id_for(class_qualified_name.as_str())
                    .unwrap_or_else(|| scope.source_id.clone()),
                file_id: scope.file_id.clone(),
                module: scope.module.clone(),
                class_qualified_name: Some(class_qualified_name.clone()),
            };
            for base in &class_def.bases {
                collect_calls_from_expr(base, file, &class_scope, alias_map, index, calls);
            }
            for decorator in &class_def.decorator_list {
                collect_calls_from_expr(decorator, file, &class_scope, alias_map, index, calls);
            }
            for child in &class_def.body {
                collect_calls_from_stmt(child, file, &class_scope, alias_map, index, calls);
            }
        }
        ast::Stmt::FunctionDef(function_def) => {
            let qualified_name = scope
                .class_qualified_name
                .as_ref()
                .map(|class| qualified_child_name(class, function_def.name.as_str()))
                .unwrap_or_else(|| {
                    qualified_child_name(file.module.as_str(), function_def.name.as_str())
                });
            let function_scope = Scope {
                source_id: index
                    .id_for(qualified_name.as_str())
                    .unwrap_or_else(|| scope.source_id.clone()),
                file_id: scope.file_id.clone(),
                module: scope.module.clone(),
                class_qualified_name: scope.class_qualified_name.clone(),
            };
            for decorator in &function_def.decorator_list {
                collect_calls_from_expr(decorator, file, &function_scope, alias_map, index, calls);
            }
            if let Some(returns) = &function_def.returns {
                collect_calls_from_expr(returns, file, &function_scope, alias_map, index, calls);
            }
            for child in &function_def.body {
                collect_calls_from_stmt(child, file, &function_scope, alias_map, index, calls);
            }
        }
        ast::Stmt::AsyncFunctionDef(function_def) => {
            let qualified_name = scope
                .class_qualified_name
                .as_ref()
                .map(|class| qualified_child_name(class, function_def.name.as_str()))
                .unwrap_or_else(|| {
                    qualified_child_name(file.module.as_str(), function_def.name.as_str())
                });
            let function_scope = Scope {
                source_id: index
                    .id_for(qualified_name.as_str())
                    .unwrap_or_else(|| scope.source_id.clone()),
                file_id: scope.file_id.clone(),
                module: scope.module.clone(),
                class_qualified_name: scope.class_qualified_name.clone(),
            };
            for decorator in &function_def.decorator_list {
                collect_calls_from_expr(decorator, file, &function_scope, alias_map, index, calls);
            }
            if let Some(returns) = &function_def.returns {
                collect_calls_from_expr(returns, file, &function_scope, alias_map, index, calls);
            }
            for child in &function_def.body {
                collect_calls_from_stmt(child, file, &function_scope, alias_map, index, calls);
            }
        }
        ast::Stmt::Return(stmt) => {
            walk_optional_expr(stmt.value.as_deref(), file, scope, alias_map, index, calls)
        }
        ast::Stmt::Delete(stmt) => walk_exprs(&stmt.targets, file, scope, alias_map, index, calls),
        ast::Stmt::Assign(stmt) => {
            walk_exprs(&stmt.targets, file, scope, alias_map, index, calls);
            collect_calls_from_expr(&stmt.value, file, scope, alias_map, index, calls);
        }
        ast::Stmt::TypeAlias(stmt) => {
            collect_calls_from_expr(&stmt.name, file, scope, alias_map, index, calls);
            collect_calls_from_expr(&stmt.value, file, scope, alias_map, index, calls);
        }
        ast::Stmt::AugAssign(stmt) => {
            collect_calls_from_expr(&stmt.target, file, scope, alias_map, index, calls);
            collect_calls_from_expr(&stmt.value, file, scope, alias_map, index, calls);
        }
        ast::Stmt::AnnAssign(stmt) => {
            collect_calls_from_expr(&stmt.target, file, scope, alias_map, index, calls);
            collect_calls_from_expr(&stmt.annotation, file, scope, alias_map, index, calls);
            walk_optional_expr(stmt.value.as_deref(), file, scope, alias_map, index, calls);
        }
        ast::Stmt::For(stmt) => {
            collect_calls_from_expr(&stmt.target, file, scope, alias_map, index, calls);
            collect_calls_from_expr(&stmt.iter, file, scope, alias_map, index, calls);
            walk_stmt_blocks(
                &[&stmt.body, &stmt.orelse],
                file,
                scope,
                alias_map,
                index,
                calls,
            );
        }
        ast::Stmt::AsyncFor(stmt) => {
            collect_calls_from_expr(&stmt.target, file, scope, alias_map, index, calls);
            collect_calls_from_expr(&stmt.iter, file, scope, alias_map, index, calls);
            walk_stmt_blocks(
                &[&stmt.body, &stmt.orelse],
                file,
                scope,
                alias_map,
                index,
                calls,
            );
        }
        ast::Stmt::While(stmt) => {
            collect_calls_from_expr(&stmt.test, file, scope, alias_map, index, calls);
            walk_stmt_blocks(
                &[&stmt.body, &stmt.orelse],
                file,
                scope,
                alias_map,
                index,
                calls,
            );
        }
        ast::Stmt::If(stmt) => {
            collect_calls_from_expr(&stmt.test, file, scope, alias_map, index, calls);
            walk_stmt_blocks(
                &[&stmt.body, &stmt.orelse],
                file,
                scope,
                alias_map,
                index,
                calls,
            );
        }
        ast::Stmt::With(stmt) => {
            for item in &stmt.items {
                collect_calls_from_expr(&item.context_expr, file, scope, alias_map, index, calls);
                walk_optional_expr(
                    item.optional_vars.as_deref(),
                    file,
                    scope,
                    alias_map,
                    index,
                    calls,
                );
            }
            walk_stmt_blocks(&[&stmt.body], file, scope, alias_map, index, calls);
        }
        ast::Stmt::AsyncWith(stmt) => {
            for item in &stmt.items {
                collect_calls_from_expr(&item.context_expr, file, scope, alias_map, index, calls);
                walk_optional_expr(
                    item.optional_vars.as_deref(),
                    file,
                    scope,
                    alias_map,
                    index,
                    calls,
                );
            }
            walk_stmt_blocks(&[&stmt.body], file, scope, alias_map, index, calls);
        }
        ast::Stmt::Match(stmt) => {
            collect_calls_from_expr(&stmt.subject, file, scope, alias_map, index, calls);
            for case in &stmt.cases {
                walk_optional_expr(case.guard.as_deref(), file, scope, alias_map, index, calls);
                walk_stmt_blocks(&[&case.body], file, scope, alias_map, index, calls);
            }
        }
        ast::Stmt::Raise(stmt) => {
            walk_optional_expr(stmt.exc.as_deref(), file, scope, alias_map, index, calls);
            walk_optional_expr(stmt.cause.as_deref(), file, scope, alias_map, index, calls);
        }
        ast::Stmt::Try(stmt) => {
            walk_stmt_blocks(
                &[&stmt.body, &stmt.orelse, &stmt.finalbody],
                file,
                scope,
                alias_map,
                index,
                calls,
            );
            walk_handlers(&stmt.handlers, file, scope, alias_map, index, calls);
        }
        ast::Stmt::TryStar(stmt) => {
            walk_stmt_blocks(
                &[&stmt.body, &stmt.orelse, &stmt.finalbody],
                file,
                scope,
                alias_map,
                index,
                calls,
            );
            walk_handlers(&stmt.handlers, file, scope, alias_map, index, calls);
        }
        ast::Stmt::Assert(stmt) => {
            collect_calls_from_expr(&stmt.test, file, scope, alias_map, index, calls);
            walk_optional_expr(stmt.msg.as_deref(), file, scope, alias_map, index, calls);
        }
        ast::Stmt::Expr(stmt) => {
            collect_calls_from_expr(&stmt.value, file, scope, alias_map, index, calls)
        }
        ast::Stmt::Import(_)
        | ast::Stmt::ImportFrom(_)
        | ast::Stmt::Global(_)
        | ast::Stmt::Nonlocal(_)
        | ast::Stmt::Pass(_)
        | ast::Stmt::Break(_)
        | ast::Stmt::Continue(_) => {}
    }
}

fn collect_calls_from_expr(
    expr: &ast::Expr,
    file: &ParsedPythonFile,
    scope: &Scope,
    alias_map: &BTreeMap<String, String>,
    index: &SymbolIndex,
    calls: &mut Vec<CodeCall>,
) {
    match expr {
        ast::Expr::BoolOp(expr) => walk_exprs(&expr.values, file, scope, alias_map, index, calls),
        ast::Expr::NamedExpr(expr) => {
            collect_calls_from_expr(&expr.target, file, scope, alias_map, index, calls);
            collect_calls_from_expr(&expr.value, file, scope, alias_map, index, calls);
        }
        ast::Expr::BinOp(expr) => {
            collect_calls_from_expr(&expr.left, file, scope, alias_map, index, calls);
            collect_calls_from_expr(&expr.right, file, scope, alias_map, index, calls);
        }
        ast::Expr::UnaryOp(expr) => {
            collect_calls_from_expr(&expr.operand, file, scope, alias_map, index, calls)
        }
        ast::Expr::Lambda(expr) => {
            collect_calls_from_expr(&expr.body, file, scope, alias_map, index, calls)
        }
        ast::Expr::IfExp(expr) => {
            collect_calls_from_expr(&expr.test, file, scope, alias_map, index, calls);
            collect_calls_from_expr(&expr.body, file, scope, alias_map, index, calls);
            collect_calls_from_expr(&expr.orelse, file, scope, alias_map, index, calls);
        }
        ast::Expr::Dict(expr) => {
            for key in &expr.keys {
                walk_optional_expr(key.as_ref(), file, scope, alias_map, index, calls);
            }
            walk_exprs(&expr.values, file, scope, alias_map, index, calls);
        }
        ast::Expr::Set(expr) => walk_exprs(&expr.elts, file, scope, alias_map, index, calls),
        ast::Expr::ListComp(expr) => {
            collect_calls_from_expr(&expr.elt, file, scope, alias_map, index, calls);
            walk_comprehensions(&expr.generators, file, scope, alias_map, index, calls);
        }
        ast::Expr::SetComp(expr) => {
            collect_calls_from_expr(&expr.elt, file, scope, alias_map, index, calls);
            walk_comprehensions(&expr.generators, file, scope, alias_map, index, calls);
        }
        ast::Expr::DictComp(expr) => {
            collect_calls_from_expr(&expr.key, file, scope, alias_map, index, calls);
            collect_calls_from_expr(&expr.value, file, scope, alias_map, index, calls);
            walk_comprehensions(&expr.generators, file, scope, alias_map, index, calls);
        }
        ast::Expr::GeneratorExp(expr) => {
            collect_calls_from_expr(&expr.elt, file, scope, alias_map, index, calls);
            walk_comprehensions(&expr.generators, file, scope, alias_map, index, calls);
        }
        ast::Expr::Await(expr) => {
            collect_calls_from_expr(&expr.value, file, scope, alias_map, index, calls)
        }
        ast::Expr::Yield(expr) => {
            walk_optional_expr(expr.value.as_deref(), file, scope, alias_map, index, calls)
        }
        ast::Expr::YieldFrom(expr) => {
            collect_calls_from_expr(&expr.value, file, scope, alias_map, index, calls)
        }
        ast::Expr::Compare(expr) => {
            collect_calls_from_expr(&expr.left, file, scope, alias_map, index, calls);
            walk_exprs(&expr.comparators, file, scope, alias_map, index, calls);
        }
        ast::Expr::Call(expr) => {
            if let Some(target) = expr_path(&expr.func) {
                calls.push(CodeCall {
                    source_id: scope.source_id.clone(),
                    source_file_id: scope.file_id.clone(),
                    target: target.clone(),
                    line: offset_to_line(&file.source, expr.range.start()),
                    target_id: resolve_reference(&target, scope, alias_map, index),
                });
            }
            collect_calls_from_expr(&expr.func, file, scope, alias_map, index, calls);
            walk_exprs(&expr.args, file, scope, alias_map, index, calls);
            for keyword in &expr.keywords {
                collect_calls_from_expr(&keyword.value, file, scope, alias_map, index, calls);
            }
        }
        ast::Expr::FormattedValue(expr) => {
            collect_calls_from_expr(&expr.value, file, scope, alias_map, index, calls);
            walk_optional_expr(
                expr.format_spec.as_deref(),
                file,
                scope,
                alias_map,
                index,
                calls,
            );
        }
        ast::Expr::JoinedStr(expr) => {
            walk_exprs(&expr.values, file, scope, alias_map, index, calls)
        }
        ast::Expr::Constant(_) => {}
        ast::Expr::Attribute(expr) => {
            collect_calls_from_expr(&expr.value, file, scope, alias_map, index, calls)
        }
        ast::Expr::Subscript(expr) => {
            collect_calls_from_expr(&expr.value, file, scope, alias_map, index, calls);
            collect_calls_from_expr(&expr.slice, file, scope, alias_map, index, calls);
        }
        ast::Expr::Starred(expr) => {
            collect_calls_from_expr(&expr.value, file, scope, alias_map, index, calls)
        }
        ast::Expr::Name(_) => {}
        ast::Expr::List(expr) => walk_exprs(&expr.elts, file, scope, alias_map, index, calls),
        ast::Expr::Tuple(expr) => walk_exprs(&expr.elts, file, scope, alias_map, index, calls),
        ast::Expr::Slice(expr) => {
            walk_optional_expr(expr.lower.as_deref(), file, scope, alias_map, index, calls);
            walk_optional_expr(expr.upper.as_deref(), file, scope, alias_map, index, calls);
            walk_optional_expr(expr.step.as_deref(), file, scope, alias_map, index, calls);
        }
    }
}

fn walk_exprs(
    exprs: &[ast::Expr],
    file: &ParsedPythonFile,
    scope: &Scope,
    alias_map: &BTreeMap<String, String>,
    index: &SymbolIndex,
    calls: &mut Vec<CodeCall>,
) {
    for expr in exprs {
        collect_calls_from_expr(expr, file, scope, alias_map, index, calls);
    }
}

fn walk_optional_expr(
    expr: Option<&ast::Expr>,
    file: &ParsedPythonFile,
    scope: &Scope,
    alias_map: &BTreeMap<String, String>,
    index: &SymbolIndex,
    calls: &mut Vec<CodeCall>,
) {
    if let Some(expr) = expr {
        collect_calls_from_expr(expr, file, scope, alias_map, index, calls);
    }
}

fn walk_comprehensions(
    comprehensions: &[ast::Comprehension],
    file: &ParsedPythonFile,
    scope: &Scope,
    alias_map: &BTreeMap<String, String>,
    index: &SymbolIndex,
    calls: &mut Vec<CodeCall>,
) {
    for comprehension in comprehensions {
        collect_calls_from_expr(&comprehension.target, file, scope, alias_map, index, calls);
        collect_calls_from_expr(&comprehension.iter, file, scope, alias_map, index, calls);
        walk_exprs(&comprehension.ifs, file, scope, alias_map, index, calls);
    }
}

fn walk_stmt_blocks(
    blocks: &[&Vec<ast::Stmt>],
    file: &ParsedPythonFile,
    scope: &Scope,
    alias_map: &BTreeMap<String, String>,
    index: &SymbolIndex,
    calls: &mut Vec<CodeCall>,
) {
    for block in blocks {
        for stmt in *block {
            collect_calls_from_stmt(stmt, file, scope, alias_map, index, calls);
        }
    }
}

fn walk_handlers(
    handlers: &[ast::ExceptHandler],
    file: &ParsedPythonFile,
    scope: &Scope,
    alias_map: &BTreeMap<String, String>,
    index: &SymbolIndex,
    calls: &mut Vec<CodeCall>,
) {
    for handler in handlers {
        let ast::ExceptHandler::ExceptHandler(handler) = handler;
        walk_optional_expr(
            handler.type_.as_deref(),
            file,
            scope,
            alias_map,
            index,
            calls,
        );
        walk_stmt_blocks(&[&handler.body], file, scope, alias_map, index, calls);
    }
}

fn stmt_child_blocks(stmt: &ast::Stmt) -> Vec<Option<&Vec<ast::Stmt>>> {
    match stmt {
        ast::Stmt::For(stmt) => vec![Some(&stmt.body), Some(&stmt.orelse)],
        ast::Stmt::AsyncFor(stmt) => vec![Some(&stmt.body), Some(&stmt.orelse)],
        ast::Stmt::While(stmt) => vec![Some(&stmt.body), Some(&stmt.orelse)],
        ast::Stmt::If(stmt) => vec![Some(&stmt.body), Some(&stmt.orelse)],
        ast::Stmt::With(stmt) => vec![Some(&stmt.body)],
        ast::Stmt::AsyncWith(stmt) => vec![Some(&stmt.body)],
        ast::Stmt::Match(stmt) => stmt.cases.iter().map(|case| Some(&case.body)).collect(),
        ast::Stmt::Try(stmt) => vec![Some(&stmt.body), Some(&stmt.orelse), Some(&stmt.finalbody)],
        ast::Stmt::TryStar(stmt) => {
            vec![Some(&stmt.body), Some(&stmt.orelse), Some(&stmt.finalbody)]
        }
        _ => Vec::new(),
    }
}

fn expr_path(expr: &ast::Expr) -> Option<String> {
    match expr {
        ast::Expr::Name(expr) => Some(expr.id.to_string()),
        ast::Expr::Attribute(expr) => {
            expr_path(&expr.value).map(|base| format!("{base}.{}", expr.attr))
        }
        ast::Expr::Subscript(expr) => expr_path(&expr.value),
        ast::Expr::Call(expr) => expr_path(&expr.func),
        _ => None,
    }
}

fn resolve_reference(
    target: &str,
    scope: &Scope,
    alias_map: &BTreeMap<String, String>,
    index: &SymbolIndex,
) -> Option<String> {
    let parts = target.split('.').collect::<Vec<_>>();
    if let Some(class_qualified_name) = &scope.class_qualified_name {
        if matches!(parts.first(), Some(&"self" | &"cls")) && parts.len() > 1 {
            let candidate = format!("{}.{}", class_qualified_name, parts[1..].join("."));
            if let Some(id) = index.id_for_entity(candidate.as_str()) {
                return Some(id);
            }
        }
    }
    if let Some(alias) = parts.first().and_then(|first| alias_map.get(*first)) {
        let candidate = if parts.len() == 1 {
            alias.clone()
        } else {
            format!("{}.{}", alias, parts[1..].join("."))
        };
        if let Some(id) = index.id_for_entity(candidate.as_str()) {
            return Some(id);
        }
    }
    if let Some(id) = index.id_for_entity(target) {
        return Some(id);
    }
    if parts.len() == 1 {
        if let Some(qualified) = index.module_symbol(&scope.module, target) {
            return index.id_for(qualified.as_str());
        }
        if let Some(id) = index.unique_symbol_id_for_name(target) {
            return Some(id);
        }
    }
    let same_module_candidate = format!("{}.{}", scope.module, target);
    if index.has_symbol(same_module_candidate.as_str()) {
        return index.id_for(same_module_candidate.as_str());
    }
    None
}

fn resolve_import_from_module(
    current_module: &str,
    is_package_init: bool,
    imported_module: Option<&str>,
    level: usize,
) -> String {
    if level == 0 {
        return imported_module.unwrap_or_default().to_string();
    }
    let mut base_parts = current_module.split('.').collect::<Vec<_>>();
    if !is_package_init && !base_parts.is_empty() {
        base_parts.pop();
    }
    for _ in 1..level {
        if !base_parts.is_empty() {
            base_parts.pop();
        }
    }
    if let Some(imported_module) = imported_module {
        if !imported_module.is_empty() {
            base_parts.extend(imported_module.split('.'));
        }
    }
    base_parts.join(".")
}

fn line_count(text: &str) -> usize {
    text.lines().count().max(1)
}

fn offset_to_line(text: &str, offset: rustpython_parser::text_size::TextSize) -> usize {
    let offset = u32::from(offset) as usize;
    let offset = offset.min(text.len());
    text.as_bytes()[..offset]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SinkMode;

    fn config(root: &Path) -> RepoConfig {
        let mut sources = BTreeMap::new();
        sources.insert(
            "python".to_string(),
            SourceConfig {
                required: true,
                include: vec!["pkg/**/*.py".into()],
                exclude: vec![],
                symbols: true,
                edges: Some("codegraphcontext".into()),
                ..Default::default()
            },
        );
        RepoConfig {
            manifest_path: root.join("sources.jsonl"),
            bib_path: root.join("references.bib"),
            tex_root: root.join("tex"),
            pdf_root: root.join("pdf"),
            generated_docs_root: root.join("generated"),
            registry_path: None,
            parsed_root: None,
            neo4j_export_root: None,
            memory_state_root: None,
            sink: SinkMode::Neo4j,
            graphify_rebuild_command: None,
            download_pdfs: false,
            relevance_tags: vec![],
            semantic_scholar: None,
            authority_tiers: None,
            project: Some(crate::config::ProjectConfig {
                id: "fixture".into(),
                name: "Fixture".into(),
                root: root.to_path_buf(),
            }),
            sources,
            representation: None,
            backends: None,
            storage: None,
        }
    }

    #[test]
    fn extracts_ast_imports_and_resolved_calls() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("pkg")).unwrap();
        fs::write(dir.path().join("pkg/__init__.py"), "").unwrap();
        fs::write(
            dir.path().join("pkg/a.py"),
            r#"
from .b import helper

class Runner:
    def close(self):
        return helper()

    def run(self):
        return self.close()
"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("pkg/b.py"),
            r#"
def helper():
    return 1
"#,
        )
        .unwrap();

        let graph = build_python_code_graph(&config(dir.path())).unwrap();

        assert!(graph
            .files
            .iter()
            .any(|file| file.repo_path == "pkg/a.py" && file.module == "pkg.a"));
        assert!(graph
            .symbols
            .iter()
            .any(|symbol| symbol.qualified_name == "pkg.a.Runner.run"));
        assert!(graph.imports.iter().any(|import| {
            import.imported == "pkg.b.helper"
                && import.target_id.as_deref() == Some("code_symbol:pkg.b.helper")
        }));
        assert!(graph.calls.iter().any(|call| {
            call.source_id == "code_symbol:pkg.a.Runner.close"
                && call.target == "helper"
                && call.target_id.as_deref() == Some("code_symbol:pkg.b.helper")
        }));
        assert!(graph.calls.iter().any(|call| {
            call.source_id == "code_symbol:pkg.a.Runner.run"
                && call.target == "self.close"
                && call.target_id.as_deref() == Some("code_symbol:pkg.a.Runner.close")
        }));
    }
}
