use litkg_core::schema::harness::GraphSnapshot;
use litkg_core::{
    CanonicalNode, NodeKind, Provenance, ProvenanceSpan, StableId,
};
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn verifies_graph_snapshot_idempotency() {
    let id = StableId::new("paper:doi:10.1000/182");
    let provenance = Provenance {
        source_id: "docs/references.bib".to_string(),
        source_hash: "abc123hash".to_string(),
        span: ProvenanceSpan::LineRange { start: 10, end: 20 },
        adapter_name: "bibtex-adapter".to_string(),
        adapter_version: "0.1.0".to_string(),
        ingested_at: "2026-04-30T12:00:00Z".to_string(),
        confidence: 1.0,
    };

    let node = CanonicalNode {
        id: id.clone(),
        kind: NodeKind::Paper,
        label: "A Unified Knowledge Graph".to_string(),
        aliases: vec![],
        properties: json!({"year": 2026}),
        provenance: vec![provenance],
        schema_version: "0.1.0".to_string(),
    };

    let mut nodes = BTreeMap::new();
    nodes.insert(id.to_string(), node);

    let snapshot1 = GraphSnapshot {
        nodes: nodes.clone(),
        edges: BTreeMap::new(),
    };

    let snapshot2 = GraphSnapshot {
        nodes,
        edges: BTreeMap::new(),
    };

    snapshot1.assert_equal(&snapshot2);
}
