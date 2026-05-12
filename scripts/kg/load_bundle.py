#!/usr/bin/env python3
"""Load a litkg Neo4j JSONL export bundle into the live Neo4j runtime."""

from __future__ import annotations

import argparse
import base64
import json
import os
import re
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import Request, urlopen


LABEL_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
REL_RE = re.compile(r"^[A-Z][A-Z0-9_]*$")


def load_dotenv(dotenv_path: Path) -> None:
    if not dotenv_path.exists():
        return
    for raw_line in dotenv_path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        os.environ.setdefault(key.strip(), value.strip().strip('"').strip("'"))


def repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def derive_http_url(neo4j_uri: str) -> str:
    parsed = urlparse(neo4j_uri)
    host = parsed.hostname or "localhost"
    return f"http://{host}:7474"


class Neo4jHTTP:
    def __init__(
        self, base_url: str, database: str, username: str, password: str
    ) -> None:
        auth = base64.b64encode(f"{username}:{password}".encode("utf-8")).decode(
            "ascii"
        )
        self.tx_url = f"{base_url.rstrip('/')}/db/{database}/tx/commit"
        self.auth_header = f"Basic {auth}"

    def query(self, statement: str, parameters: dict[str, Any] | None = None) -> None:
        request = Request(
            self.tx_url,
            data=json.dumps(
                {
                    "statements": [
                        {
                            "statement": statement,
                            "parameters": parameters or {},
                            "resultDataContents": ["row"],
                        }
                    ]
                }
            ).encode("utf-8"),
            headers={
                "Authorization": self.auth_header,
                "Content-Type": "application/json",
                "Accept": "application/json",
            },
            method="POST",
        )
        with urlopen(request) as response:
            body = json.loads(response.read().decode("utf-8"))
        errors = body.get("errors", [])
        if errors:
            raise RuntimeError(f"Neo4j error: {errors[0]}")


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    with path.open(encoding="utf-8") as handle:
        for line_number, raw_line in enumerate(handle, start=1):
            line = raw_line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as exc:
                raise RuntimeError(f"{path}:{line_number}: invalid JSONL row") from exc
    return rows


def safe_labels(labels: list[str]) -> list[str]:
    safe = []
    for label in labels:
        if not LABEL_RE.match(label):
            raise RuntimeError(f"Unsafe Neo4j label in export bundle: {label!r}")
        safe.append(label)
    return safe or ["LitkgNode"]


def safe_rel_type(rel_type: str) -> str:
    if not REL_RE.match(rel_type):
        raise RuntimeError(f"Unsafe Neo4j relationship type in export bundle: {rel_type!r}")
    return rel_type


def batched(rows: list[dict[str, Any]], size: int = 500):
    for index in range(0, len(rows), size):
        yield rows[index : index + size]


def load_nodes(client: Neo4jHTTP, nodes: list[dict[str, Any]]) -> int:
    grouped: dict[tuple[str, ...], list[dict[str, Any]]] = defaultdict(list)
    for node in nodes:
        node_id = node["id"]
        props = dict(node.get("properties") or {})
        props["id"] = node_id
        props["litkg_id"] = node_id
        labels = tuple(safe_labels(list(node.get("labels") or [])))
        grouped[labels].append({"id": node_id, "props": props})

    loaded = 0
    for labels, rows in grouped.items():
        all_labels = tuple(dict.fromkeys(("LitkgNode", *labels)))
        label_clause = ":" + ":".join(all_labels)
        statement = f"""
        UNWIND $rows AS row
        MERGE (n{label_clause} {{litkg_id: row.id}})
        SET n += row.props
        RETURN count(n)
        """
        for chunk in batched(rows):
            client.query(statement, {"rows": chunk})
            loaded += len(chunk)
    return loaded


def load_edges(client: Neo4jHTTP, edges: list[dict[str, Any]]) -> int:
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for edge in edges:
        rel_type = safe_rel_type(edge["rel_type"])
        grouped[rel_type].append(
            {
                "source": edge["source"],
                "target": edge["target"],
                "props": dict(edge.get("properties") or {}),
            }
        )

    loaded = 0
    for rel_type, rows in grouped.items():
        statement = f"""
        UNWIND $rows AS row
        MATCH (source:LitkgNode {{litkg_id: row.source}})
        MATCH (target:LitkgNode {{litkg_id: row.target}})
        MERGE (source)-[r:{rel_type}]->(target)
        SET r += row.props
        RETURN count(r)
        """
        for chunk in batched(rows):
            client.query(statement, {"rows": chunk})
            loaded += len(chunk)
    return loaded


def prepare_schema(client: Neo4jHTTP) -> None:
    client.query(
        """
        MATCH (n)
        WHERE n.litkg_id IS NOT NULL
        SET n:LitkgNode
        RETURN count(n)
        """
    )
    client.query(
        """
        CREATE INDEX litkg_node_litkg_id IF NOT EXISTS
        FOR (n:LitkgNode) ON (n.litkg_id)
        """
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle-root", required=True)
    args = parser.parse_args()

    host_root = repo_root()
    load_dotenv(host_root / ".env.example")
    load_dotenv(host_root / ".env")

    bundle_root = Path(args.bundle_root)
    nodes_path = bundle_root / "nodes.jsonl"
    edges_path = bundle_root / "edges.jsonl"
    if not nodes_path.is_file() or not edges_path.is_file():
        print(
            f"Expected nodes.jsonl and edges.jsonl under {bundle_root}",
            file=sys.stderr,
        )
        return 2

    neo4j_uri = os.environ.get("NEO4J_URI", "bolt://localhost:7687")
    neo4j_http_url = os.environ.get("NEO4J_HTTP_URL", derive_http_url(neo4j_uri))
    neo4j_database = os.environ.get("NEO4J_DATABASE", "neo4j")
    neo4j_username = os.environ.get(
        "NEO4J_USERNAME", os.environ.get("NEO4J_USER", "neo4j")
    )
    neo4j_password = os.environ.get("NEO4J_PASSWORD", "litkglocal")

    client = Neo4jHTTP(neo4j_http_url, neo4j_database, neo4j_username, neo4j_password)
    try:
        nodes = read_jsonl(nodes_path)
        edges = read_jsonl(edges_path)
        prepare_schema(client)
        node_count = load_nodes(client, nodes)
        edge_count = load_edges(client, edges)
    except (HTTPError, URLError, OSError, RuntimeError) as exc:
        print(f"Failed to load Neo4j bundle: {exc}", file=sys.stderr)
        return 1

    print(f"Loaded {node_count} nodes and {edge_count} edges into Neo4j.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
