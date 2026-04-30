use assert_cmd::Command;
use litkg_core::{
    write_parsed_papers, write_registry, DocumentKind, DownloadMode, PaperFigure, PaperSection,
    PaperSourceRecord, PaperTable, ParseStatus, ParsedPaper, SourceKind,
};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn write_test_config(root: &Path) -> PathBuf {
    let manifest_path = root.join("sources.jsonl");
    let bib_path = root.join("references.bib");
    let tex_root = root.join("tex");
    let pdf_root = root.join("pdf");
    let generated_docs_root = root.join("generated");
    let parsed_root = generated_docs_root.join("parsed");
    let registry_path = generated_docs_root.join("registry.jsonl");

    fs::create_dir_all(tex_root.join("demo-tex")).unwrap();
    fs::create_dir_all(&pdf_root).unwrap();
    fs::create_dir_all(&generated_docs_root).unwrap();
    fs::write(
        &manifest_path,
        "{\"title\":\"Manifest Demo Paper\",\"arxiv_id\":\"2501.12345\",\"tex_dir\":\"demo-tex\",\"pdf_file\":\"demo-paper.pdf\"}\n",
    )
    .unwrap();
    fs::write(
        &bib_path,
        "@article{demo2025paper,\n  title = {Bib Demo Paper},\n  author = {Dana Demo},\n  year = {2025},\n  eprint = {2501.12345}\n}\n",
    )
    .unwrap();
    fs::write(
        tex_root.join("demo-tex").join("main.tex"),
        "\\title{Parsed Demo Paper}",
    )
    .unwrap();
    fs::write(pdf_root.join("demo-paper.pdf"), b"pdf").unwrap();

    let registry = vec![PaperSourceRecord {
        paper_id: "demo2025paper".into(),
        citation_key: Some("demo2025paper".into()),
        title: "Demo Paper".into(),
        authors: vec!["Dana Demo".into()],
        year: Some("2025".into()),
        arxiv_id: Some("2501.12345".into()),
        doi: None,
        url: Some("https://example.com/demo-paper".into()),
        tex_dir: Some("demo-paper".into()),
        pdf_file: Some("demo-paper.pdf".into()),
        source_kind: SourceKind::ManifestAndBib,
        download_mode: DownloadMode::ManifestSourcePlusPdf,
        has_local_tex: true,
        has_local_pdf: true,
        parse_status: ParseStatus::Parsed,
        semantic_scholar: None,
    }];
    write_registry(&registry_path, &registry).unwrap();

    let parsed = vec![ParsedPaper {
        kind: DocumentKind::Literature,
        metadata: PaperSourceRecord {
            title: "Parsed Demo Paper".into(),
            ..registry[0].clone()
        },
        abstract_text: Some("A demo abstract about retrieval.".into()),
        sections: vec![PaperSection {
            level: 1,
            title: "Results".into(),
            content: "Searchable section body.".into(),
        }],
        figures: vec![PaperFigure {
            caption: "Caption token figure".into(),
        }],
        tables: vec![PaperTable {
            caption: "Caption token table".into(),
        }],
        citations: vec!["ref2024alpha".into()],
        citation_references: vec![],
        provenance: vec!["demo.tex".into()],
    }];
    write_parsed_papers(&parsed_root, &parsed).unwrap();

    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            "manifest_path = \"{}\"\n\
bib_path = \"{}\"\n\
tex_root = \"{}\"\n\
pdf_root = \"{}\"\n\
generated_docs_root = \"{}\"\n\
registry_path = \"{}\"\n\
parsed_root = \"{}\"\n\
neo4j_export_root = \"{}\"\n\
sink = \"graphify\"\n\
download_pdfs = true\n\
relevance_tags = [\"retrieval\"]\n",
            manifest_path.display(),
            bib_path.display(),
            tex_root.display(),
            pdf_root.display(),
            generated_docs_root.display(),
            registry_path.display(),
            parsed_root.display(),
            generated_docs_root.join("neo4j-export").display(),
        ),
    )
    .unwrap();

    config_path
}

fn write_agents_scaffold(root: &Path) {
    let agents = root.join(".agents");
    fs::create_dir_all(agents.join("skills/context-pack-builder")).unwrap();
    fs::write(
        root.join("AGENTS.md"),
        "# Test Agent Guidance\n\nUse context packs for scaffold work.\n",
    )
    .unwrap();
    fs::write(
        agents.join("AGENTS_INTERNAL_DB.md"),
        "# Internal DB\n\nContext-pack work is part of the backbone.\n",
    )
    .unwrap();
    fs::write(
        agents.join("issues.toml"),
        "meta = { updated_on = \"2026-04-30\" }\n\n\
[[issues]]\n\
id = \"ISSUE-0048\"\n\
title = \"Implement context-pack CLI before MCP\"\n\
summary = \"Build evidence bundles for agents before MCP wrapping.\"\n\
priority = \"critical\"\n\
status = \"open\"\n",
    )
    .unwrap();
    fs::write(
        agents.join("todos.toml"),
        "meta = { updated_on = \"2026-04-30\" }\n\n\
[[todos]]\n\
id = \"TODO-0030\"\n\
title = \"Implement context-pack CLI evidence-bundle generation before MCP wrapping\"\n\
issue_ids = [\"ISSUE-0048\"]\n\
priority = \"critical\"\n\
status = \"pending\"\n\
loc_min = 200\n\
loc_expected = 400\n\
loc_max = 800\n",
    )
    .unwrap();
    fs::write(
        agents.join("resolved.toml"),
        "meta = { updated_on = \"2026-04-30\" }\nresolved_issues = []\nresolved_todos = []\n",
    )
    .unwrap();
    fs::write(
        agents.join("skills/context-pack-builder/SKILL.md"),
        "---\nname: context-pack-builder\ndescription: Use when building context packs for agent handoff.\n---\n\n# Context Pack Builder\n",
    )
    .unwrap();
}

fn write_metadata_only_config(root: &Path) -> PathBuf {
    let manifest_path = root.join("sources.jsonl");
    let bib_path = root.join("references.bib");
    let tex_root = root.join("tex");
    let pdf_root = root.join("pdf");
    let generated_docs_root = root.join("generated");
    let parsed_root = generated_docs_root.join("parsed");
    let registry_path = generated_docs_root.join("registry.jsonl");

    fs::create_dir_all(&generated_docs_root).unwrap();
    fs::create_dir_all(&parsed_root).unwrap();
    fs::write(&manifest_path, "").unwrap();
    fs::write(&bib_path, "").unwrap();

    write_registry(
        &registry_path,
        &[PaperSourceRecord {
            paper_id: "metadata-only".into(),
            citation_key: Some("metadata2025paper".into()),
            title: "Metadata Only".into(),
            authors: vec!["Mia Metadata".into()],
            year: Some("2025".into()),
            arxiv_id: None,
            doi: None,
            url: None,
            tex_dir: Some("missing-tex".into()),
            pdf_file: Some("missing.pdf".into()),
            source_kind: SourceKind::Bib,
            download_mode: DownloadMode::MetadataOnly,
            has_local_tex: false,
            has_local_pdf: false,
            parse_status: ParseStatus::MetadataOnly,
            semantic_scholar: None,
        }],
    )
    .unwrap();

    let config_path = root.join("metadata-only.toml");
    fs::write(
        &config_path,
        format!(
            "manifest_path = \"{}\"\n\
bib_path = \"{}\"\n\
tex_root = \"{}\"\n\
pdf_root = \"{}\"\n\
generated_docs_root = \"{}\"\n\
registry_path = \"{}\"\n\
parsed_root = \"{}\"\n\
neo4j_export_root = \"{}\"\n\
sink = \"neo4j\"\n\
download_pdfs = false\n\
relevance_tags = []\n",
            manifest_path.display(),
            bib_path.display(),
            tex_root.display(),
            pdf_root.display(),
            generated_docs_root.display(),
            registry_path.display(),
            parsed_root.display(),
            generated_docs_root.join("neo4j-export").display(),
        ),
    )
    .unwrap();

    config_path
}

fn write_stale_snapshot_config(root: &Path) -> PathBuf {
    let manifest_path = root.join("sources.jsonl");
    let bib_path = root.join("references.bib");
    let tex_root = root.join("tex");
    let pdf_root = root.join("pdf");
    let generated_docs_root = root.join("generated");
    let parsed_root = generated_docs_root.join("parsed");

    fs::create_dir_all(tex_root.join("paper-tex")).unwrap();
    fs::create_dir_all(&pdf_root).unwrap();
    fs::create_dir_all(&parsed_root).unwrap();
    fs::write(
        &manifest_path,
        "{\"title\":\"Current Manifest Paper\",\"arxiv_id\":\"2601.00001\",\"tex_dir\":\"paper-tex\"}\n",
    )
    .unwrap();
    fs::write(&bib_path, "").unwrap();
    fs::write(
        tex_root.join("paper-tex").join("main.tex"),
        "\\title{Parsed Paper}",
    )
    .unwrap();

    write_parsed_papers(
        &parsed_root,
        &[ParsedPaper {
            kind: DocumentKind::Literature,
            metadata: PaperSourceRecord {
                paper_id: "old2024paper".into(),
                citation_key: Some("old2024paper".into()),
                title: "Parsed Paper".into(),
                authors: vec!["Old Author".into()],
                year: Some("2024".into()),
                arxiv_id: Some("2601.00001".into()),
                doi: None,
                url: None,
                tex_dir: Some("paper-tex".into()),
                pdf_file: Some("stale.pdf".into()),
                source_kind: SourceKind::ManifestAndBib,
                download_mode: DownloadMode::ManifestSourcePlusPdf,
                has_local_tex: true,
                has_local_pdf: true,
                parse_status: ParseStatus::Parsed,
                semantic_scholar: None,
            },
            abstract_text: Some("stale parsed abstract".into()),
            sections: vec![PaperSection {
                level: 1,
                title: "Parsed Section".into(),
                content: "content".into(),
            }],
            figures: vec![],
            tables: vec![],
            citations: vec![],
            citation_references: vec![],
            provenance: vec!["paper-tex/main.tex".into()],
        }],
    )
    .unwrap();

    let config_path = root.join("stale.toml");
    fs::write(
        &config_path,
        format!(
            "manifest_path = \"{}\"\n\
bib_path = \"{}\"\n\
tex_root = \"{}\"\n\
pdf_root = \"{}\"\n\
generated_docs_root = \"{}\"\n\
registry_path = \"{}\"\n\
parsed_root = \"{}\"\n\
neo4j_export_root = \"{}\"\n\
sink = \"graphify\"\n\
download_pdfs = false\n\
relevance_tags = []\n",
            manifest_path.display(),
            bib_path.display(),
            tex_root.display(),
            pdf_root.display(),
            generated_docs_root.display(),
            generated_docs_root.join("registry.jsonl").display(),
            parsed_root.display(),
            generated_docs_root.join("neo4j-export").display(),
        ),
    )
    .unwrap();

    config_path
}

#[test]
fn stats_command_supports_json_output() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_test_config(dir.path());

    let output = Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "stats",
            "--config",
            config_path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["total_papers"], 1);
    assert_eq!(json["papers_with_parsed_content"], 1);
    assert_eq!(json["parse_status_counts"]["Parsed"], 1);
}

#[test]
fn capabilities_command_reports_json_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_test_config(dir.path());
    let generated = dir.path().join("generated");
    fs::write(generated.join("index.md"), "# Index\n").unwrap();
    fs::write(generated.join("graphify-manifest.json"), "{}\n").unwrap();
    let neo4j = generated.join("neo4j-export");
    fs::create_dir_all(&neo4j).unwrap();
    fs::write(neo4j.join("nodes.jsonl"), "{}\n").unwrap();
    fs::write(neo4j.join("edges.jsonl"), "{}\n").unwrap();

    let output = Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "capabilities",
            "--config",
            config_path.to_str().unwrap(),
            "--repo-root",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["literature_registry"]["records_total"], 1);
    assert_eq!(json["literature_registry"]["registry_generated"], true);
    assert_eq!(json["downloads"]["records_with_local_tex"], 1);
    assert_eq!(json["parsing"]["parsed_papers"], 1);
    assert_eq!(json["parsing"]["total_sections"], 1);
    assert_eq!(json["graph_outputs"]["graphify_state"], "generated");
    assert_eq!(json["graph_outputs"]["neo4j_state"], "generated");
    assert_eq!(json["graph_outputs"]["native_viewer_state"], "ready");
    assert_eq!(json["runtime"]["checked"], false);
    assert!(json["next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item.as_str().unwrap().contains("inspect-graph")));
}

#[test]
fn capabilities_command_is_read_only_for_missing_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_test_config(dir.path());
    let registry_path = dir.path().join("generated").join("registry.jsonl");
    fs::remove_file(&registry_path).unwrap();
    fs::write(
        &config_path,
        format!(
            "{}\n[semantic_scholar]\nenabled = true\napi_key_env = \"LITKG_TEST_MISSING_SEMANTIC_KEY\"\n",
            fs::read_to_string(&config_path).unwrap()
        ),
    )
    .unwrap();
    std::env::remove_var("LITKG_TEST_MISSING_SEMANTIC_KEY");

    let output = Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "capabilities",
            "--config",
            config_path.to_str().unwrap(),
            "--format",
            "text",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("litkg capability snapshot"));
    assert!(text.contains("semantic scholar: configured"));
    assert!(text.contains("sync-registry"));
    assert!(!registry_path.exists());
}

#[test]
fn capabilities_runtime_check_is_shallow_and_json_serializable() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_metadata_only_config(dir.path());

    let output = Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "capabilities",
            "--config",
            config_path.to_str().unwrap(),
            "--repo-root",
            dir.path().to_str().unwrap(),
            "--check-runtime",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["runtime"]["checked"], true);
    assert!(json["runtime"]["docker"]["state"].is_string());
    assert!(json["runtime"]["codegraphcontext"]["detail"]
        .as_str()
        .unwrap()
        .contains(".cache/kg/venvs/cgc"));
}

#[test]
fn context_pack_command_reports_stable_json_contract() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_test_config(dir.path());
    write_agents_scaffold(dir.path());

    let output = Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "context-pack",
            "--config",
            config_path.to_str().unwrap(),
            "--repo-root",
            dir.path().to_str().unwrap(),
            "--task",
            "implement context pack retrieval",
            "--budget",
            "220",
            "--profile",
            "agents-scaffold",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["profile"], "agents-scaffold");
    assert_eq!(json["active_issues"][0]["id"], "ISSUE-0048");
    assert_eq!(json["active_todos"][0]["id"], "TODO-0030");
    assert!(!json["evidence_spans"].as_array().unwrap().is_empty());
    assert_eq!(json["relevant_papers"][0]["paper_id"], "demo2025paper");
    assert!(json["missing_context_leaves"]
        .as_array()
        .unwrap()
        .iter()
        .any(|leaf| leaf["provider"] == "Context7"));
    assert!(json["verification_commands"]
        .as_array()
        .unwrap()
        .iter()
        .any(|command| command.as_str().unwrap().contains("cargo test")));
}

#[test]
fn context_pack_command_reports_text_sections() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_test_config(dir.path());
    write_agents_scaffold(dir.path());

    let output = Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "context-pack",
            "--config",
            config_path.to_str().unwrap(),
            "--repo-root",
            dir.path().to_str().unwrap(),
            "--task",
            "implement context pack retrieval",
            "--budget",
            "220",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("litkg context pack"));
    assert!(text.contains("active issues:"));
    assert!(text.contains("evidence spans:"));
    assert!(text.contains("missing context leaves:"));
    assert!(text.contains("verification commands:"));
}

#[test]
fn search_and_show_paper_commands_work_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_test_config(dir.path());

    let search_output = Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "search",
            "--config",
            config_path.to_str().unwrap(),
            "--query",
            "caption token table",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let search_json: Value = serde_json::from_slice(&search_output).unwrap();
    assert_eq!(search_json["query"], "caption token table");
    assert_eq!(search_json["total_matches"], 1);
    assert_eq!(search_json["has_more"], false);
    assert_eq!(search_json["hits"][0]["paper_id"], "demo2025paper");
    assert_eq!(
        search_json["hits"][0]["matched_fields"][0],
        "table_captions"
    );

    Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "search",
            "--config",
            config_path.to_str().unwrap(),
            "--query",
            "caption token table",
            "--limit",
            "0",
        ])
        .assert()
        .failure();

    Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "search",
            "--config",
            config_path.to_str().unwrap(),
            "--query",
            "   ",
        ])
        .assert()
        .failure();

    let show_output = Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "show-paper",
            "--config",
            config_path.to_str().unwrap(),
            "--paper",
            "Parsed Demo Paper",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let show_json: Value = serde_json::from_slice(&show_output).unwrap();
    assert_eq!(show_json["metadata"]["paper_id"], "demo2025paper");
    assert_eq!(show_json["metadata"]["title"], "Demo Paper");
    assert_eq!(show_json["metadata"]["parse_status"], "Parsed");
    assert_eq!(show_json["figure_captions"][0], "Caption token figure");
}

#[test]
fn stats_does_not_write_registry_when_building_a_read_only_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_test_config(dir.path());
    let registry_path = dir.path().join("generated").join("registry.jsonl");
    fs::remove_file(&registry_path).unwrap();

    Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "stats",
            "--config",
            config_path.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .success();

    assert!(!registry_path.exists());

    let search_output = Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "search",
            "--config",
            config_path.to_str().unwrap(),
            "--query",
            "Searchable section body",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let search_json: Value = serde_json::from_slice(&search_output).unwrap();
    assert_eq!(search_json["hits"][0]["parse_status"], "Parsed");
    assert_eq!(search_json["hits"][0]["has_local_tex"], true);
    assert_eq!(search_json["hits"][0]["has_local_pdf"], true);

    let show_output = Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "show-paper",
            "--config",
            config_path.to_str().unwrap(),
            "--paper",
            "2501.12345",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let show_json: Value = serde_json::from_slice(&show_output).unwrap();
    assert_eq!(show_json["metadata"]["parse_status"], "Parsed");
    assert_eq!(show_json["metadata"]["source_kind"], "ManifestAndBib");
    assert_eq!(
        show_json["metadata"]["download_mode"],
        "ManifestSourcePlusPdf"
    );
    assert_eq!(show_json["metadata"]["has_local_tex"], true);
    assert_eq!(show_json["metadata"]["has_local_pdf"], true);
}

#[test]
fn show_paper_json_uses_null_for_missing_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_metadata_only_config(dir.path());

    let output = Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "show-paper",
            "--config",
            config_path.to_str().unwrap(),
            "--paper",
            "metadata2025paper",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["metadata"]["parse_status"], "MetadataOnly");
    assert_eq!(json["parsed_json_path"], Value::Null);
    assert_eq!(json["materialized_markdown_path"], Value::Null);
    assert_eq!(json["local_tex_dir"], Value::Null);
    assert_eq!(json["local_pdf_path"], Value::Null);
}

#[test]
fn no_registry_fallback_does_not_resurrect_stale_parsed_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_stale_snapshot_config(dir.path());

    let show_output = Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "show-paper",
            "--config",
            config_path.to_str().unwrap(),
            "--paper",
            "2601.00001",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let show_json: Value = serde_json::from_slice(&show_output).unwrap();
    assert_eq!(show_json["metadata"]["parse_status"], "Parsed");
    assert_eq!(show_json["metadata"]["source_kind"], "Manifest");
    assert_eq!(show_json["metadata"]["download_mode"], "ManifestSource");
    assert_eq!(show_json["metadata"]["has_local_tex"], true);
    assert_eq!(show_json["metadata"]["has_local_pdf"], false);
    assert_eq!(show_json["metadata"]["citation_key"], Value::Null);

    Command::cargo_bin("litkg-cli")
        .unwrap()
        .args([
            "show-paper",
            "--config",
            config_path.to_str().unwrap(),
            "--paper",
            "old2024paper",
            "--format",
            "json",
        ])
        .assert()
        .failure();
}
