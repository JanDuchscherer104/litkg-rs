use litkg_core::markdown::parse_markdown_document;
use litkg_core::model::{DocumentKind, ParseStatus, SourceKind};
use std::fs;
use tempfile::tempdir;

#[test]
fn parses_markdown_with_frontmatter_and_sections() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.md");
    fs::write(
        &file_path,
        r#"---
title: "Project Vision"
author: "Jan Duchscherer"
date: "2026-04-30"
tags: ["vision", "kg"]
kind: Documentation
---

# Introduction
This is a knowledge backbone.

## Core Mandates
- Security
- Context Efficiency

# Implementation
Details here @zhang2026vistaslam.
"#,
    )
    .unwrap();

    let parsed = parse_markdown_document(&file_path, None, DocumentKind::Documentation).unwrap();

    assert_eq!(parsed.metadata.title, "Project Vision");
    assert_eq!(parsed.metadata.authors, vec!["Jan Duchscherer"]);
    assert_eq!(parsed.metadata.year.as_deref(), Some("2026-04-30"));
    assert_eq!(parsed.kind, DocumentKind::Documentation);
    assert_eq!(parsed.metadata.source_kind, SourceKind::Documentation);
    assert_eq!(parsed.metadata.parse_status, ParseStatus::Parsed);

    assert_eq!(parsed.sections.len(), 3);
    assert_eq!(parsed.sections[0].title, "Introduction");
    assert_eq!(parsed.sections[1].title, "Core Mandates");
    assert_eq!(parsed.sections[1].level, 2);

    assert!(parsed.citations.contains(&"zhang2026vistaslam".to_string()));
}

#[test]
fn parses_quarto_reasoning_blocks() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("transcript.qmd");
    fs::write(
        &file_path,
        r#"---
title: "Research Session"
kind: Transcript
---

# Discussion
::: {.reasoning}
I should explain the KG structure first.
:::
The KG structure is a DAG.
"#,
    )
    .unwrap();

    let parsed = parse_markdown_document(&file_path, None, DocumentKind::Transcript).unwrap();

    assert_eq!(parsed.kind, DocumentKind::Transcript);
    assert_eq!(parsed.metadata.source_kind, SourceKind::Transcript);

    let section = &parsed.sections[0];
    assert!(section
        .content
        .contains("[Reasoning: I should explain the KG structure first.]"));
    assert!(section.content.contains("The KG structure is a DAG."));
}
