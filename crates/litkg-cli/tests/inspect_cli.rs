use assert_cmd::Command;
use litkg_core::{
    write_parsed_papers, write_registry, DownloadMode, PaperFigure, PaperSection,
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

    fs::create_dir_all(&tex_root).unwrap();
    fs::create_dir_all(&pdf_root).unwrap();
    fs::create_dir_all(&generated_docs_root).unwrap();
    fs::write(&manifest_path, "").unwrap();
    fs::write(
        &bib_path,
        "@article{demo2025paper,\n  title = {Bib Demo Paper},\n  author = {Dana Demo},\n  year = {2025}\n}\n",
    )
    .unwrap();

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
    }];
    write_registry(&registry_path, &registry).unwrap();

    let parsed = vec![ParsedPaper {
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
    assert_eq!(show_json["metadata"]["title"], "Parsed Demo Paper");
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
    assert_eq!(show_json["metadata"]["parse_status"], "Parsed");
}
