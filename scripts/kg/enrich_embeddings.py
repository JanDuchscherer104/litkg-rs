#!/usr/bin/env python3
"""Augment Neo4j KG nodes with local embeddings and code-document links."""

from __future__ import annotations

import base64
import json
import math
import os
import re
import subprocess
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import Request, urlopen

EMBEDDING_DIM = 1024
CODE_LABELS = ("File", "Module", "Class", "Function")
GRAPHITI_LABELS = ("Episodic", "Entity", "Community")
TOKEN_KEEP_SHORT = {"kg", "tex", "bib"}
TOKEN_FILTER_CANDIDATE_LIMIT = 96
TOKEN_FILTER_RETURN_LIMIT = 24
PATH_REF_RE = re.compile(
    r"(?P<path>[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)+\.(?:rs|py|sh|md|qmd|toml|yml|yaml|json|jsonl))"
)


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


def list_ollama_models() -> list[str]:
    listing = subprocess.run(
        ["ollama", "list"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.splitlines()
    return [line.split()[0] for line in listing[1:] if line.strip()]


def ensure_embedding_model(model_name: str) -> None:
    installed_models = set(list_ollama_models())
    if model_name not in installed_models:
        subprocess.run(["ollama", "pull", model_name], check=True)


def derive_neo4j_http_url(neo4j_uri: str) -> str:
    parsed = urlparse(neo4j_uri)
    host = parsed.hostname or "localhost"
    return f"http://{host}:7474"


def ollama_embed_url(base_url: str) -> str:
    trimmed = base_url.rstrip("/")
    if trimmed.endswith("/v1"):
        trimmed = trimmed[:-3]
    return f"{trimmed}/api/embed"


def ollama_chat_url(base_url: str) -> str:
    trimmed = base_url.rstrip("/")
    if trimmed.endswith("/v1"):
        trimmed = trimmed[:-3]
    return f"{trimmed}/api/chat"


class Neo4jHTTP:
    def __init__(
        self, base_url: str, database: str, username: str, password: str
    ) -> None:
        auth = base64.b64encode(f"{username}:{password}".encode("utf-8")).decode(
            "ascii"
        )
        self.tx_url = f"{base_url.rstrip('/')}/db/{database}/tx/commit"
        self.auth_header = f"Basic {auth}"

    def query(
        self, statement: str, parameters: dict[str, Any] | None = None
    ) -> list[dict[str, Any]]:
        payload = {
            "statements": [
                {
                    "statement": statement,
                    "parameters": parameters or {},
                    "resultDataContents": ["row"],
                }
            ]
        }
        request = Request(
            self.tx_url,
            data=json.dumps(payload).encode("utf-8"),
            headers={
                "Authorization": self.auth_header,
                "Content-Type": "application/json",
                "Accept": "application/json",
            },
            method="POST",
        )
        try:
            with urlopen(request) as response:
                body = json.loads(response.read().decode("utf-8"))
        except (HTTPError, URLError) as exc:
            raise RuntimeError(f"Neo4j HTTP query failed: {exc}") from exc

        errors = body.get("errors", [])
        if errors:
            raise RuntimeError(f"Neo4j error: {errors[0]}")

        results = body.get("results", [])
        if not results:
            return []
        data = results[0].get("data", [])
        return [
            entry["row"][0] if len(entry["row"]) == 1 else entry["row"]
            for entry in data
        ]


@dataclass
class NodeRecord:
    node_id: int
    labels: list[str]
    props: dict[str, Any]
    kind: str
    name: str
    text: str
    tokens: set[str]
    embedding: list[float] | None = None


def pick_token_filter_model(installed_models: list[str]) -> str | None:
    preferred_models = [
        os.environ.get("KG_TOKEN_FILTER_MODEL"),
        os.environ.get("GRAPHITI_LLM_MODEL"),
    ]
    installed_set = set(installed_models)
    for model_name in preferred_models:
        if model_name and model_name in installed_set:
            return model_name
    for model_name in installed_models:
        if "embedding" not in model_name.lower():
            return model_name
    return None


def scoped_path_prefixes(raw_prefix: str | None, root: Path) -> list[str] | None:
    if raw_prefix is None or not raw_prefix.strip():
        return None
    prefix_path = Path(raw_prefix.strip())
    absolute_prefix = prefix_path if prefix_path.is_absolute() else (root / prefix_path)
    absolute_prefix = absolute_prefix.resolve()
    if not absolute_prefix.exists():
        raise RuntimeError(f"KG_CODE_PATH_PREFIX does not exist: {raw_prefix}")
    if not absolute_prefix.is_relative_to(root):
        raise RuntimeError(
            f"KG_CODE_PATH_PREFIX must live under repo root: {raw_prefix}"
        )

    relative_prefix = absolute_prefix.relative_to(root).as_posix()
    prefixes = [str(absolute_prefix)]
    if relative_prefix != ".":
        prefixes.append(relative_prefix)
    return prefixes


def split_tokens(text: str) -> list[str]:
    tokens = []
    for token in re.split(r"[^A-Za-z0-9_]+", text.lower()):
        if not token:
            continue
        if len(token) < 3 and token not in TOKEN_KEEP_SHORT:
            continue
        tokens.append(token)
    return tokens


def normalize_tokens(text: str, ignored_tokens: set[str] | None = None) -> set[str]:
    ignored = ignored_tokens or set()
    return {token for token in split_tokens(text) if token not in ignored}


def extract_json_object(text: str) -> dict[str, Any]:
    cleaned = text.strip()
    try:
        parsed = json.loads(cleaned)
    except json.JSONDecodeError:
        match = re.search(r"\{.*\}", cleaned, re.DOTALL)
        if match is None:
            raise
        parsed = json.loads(match.group(0))
    if not isinstance(parsed, dict):
        raise ValueError("Expected JSON object response.")
    return parsed


def infer_common_tokens(
    chat_url: str,
    model_name: str | None,
    code_records: list[NodeRecord],
    graphiti_records: list[NodeRecord],
) -> set[str]:
    if model_name is None:
        return set()

    code_counts: Counter[str] = Counter()
    graphiti_counts: Counter[str] = Counter()

    for record in code_records:
        code_counts.update(set(split_tokens(record.text)))
    for record in graphiti_records:
        graphiti_counts.update(set(split_tokens(record.text)))

    combined_counts = code_counts + graphiti_counts
    candidates: list[dict[str, int | str]] = []
    for token, total_hits in combined_counts.most_common(TOKEN_FILTER_CANDIDATE_LIMIT):
        if total_hits < 3:
            break
        candidates.append(
            {
                "token": token,
                "code_hits": code_counts[token],
                "graph_hits": graphiti_counts[token],
                "total_hits": total_hits,
            }
        )

    if len(candidates) < 8:
        return set()

    request = Request(
        chat_url,
        data=json.dumps(
            {
                "model": model_name,
                "stream": False,
                "format": "json",
                "options": {"temperature": 0, "seed": 0},
                "messages": [
                    {
                        "role": "system",
                        "content": (
                            "You select globally useless overlap tokens for matching "
                            "graph/document nodes to code nodes. Only choose tokens "
                            "from the provided list that are generic boilerplate, "
                            "metadata labels, or otherwise too broad to help linking. "
                            "Keep domain terms, library names, file types, graph terms, "
                            "and implementation-specific words. Return JSON with an "
                            "'ignore' array and no extra keys."
                        ),
                    },
                    {
                        "role": "user",
                        "content": json.dumps(
                            {
                                "max_ignore_tokens": TOKEN_FILTER_RETURN_LIMIT,
                                "candidates": candidates,
                            },
                            ensure_ascii=True,
                            sort_keys=True,
                        ),
                    },
                ],
            }
        ).encode("utf-8"),
        headers={"Content-Type": "application/json", "Accept": "application/json"},
        method="POST",
    )

    try:
        with urlopen(request) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except (HTTPError, URLError, json.JSONDecodeError, ValueError):
        return set()

    message = payload.get("message", {})
    content = message.get("content", "")
    if not isinstance(content, str) or not content.strip():
        return set()

    try:
        parsed = extract_json_object(content)
    except (json.JSONDecodeError, ValueError):
        return set()

    ignore = parsed.get("ignore", [])
    if not isinstance(ignore, list):
        return set()

    allowed_tokens = {entry["token"] for entry in candidates}
    return {
        token
        for token in (str(value).lower().strip() for value in ignore)
        if token in allowed_tokens
    }


def compact_json(props: dict[str, Any], keys: list[str]) -> str:
    compact: dict[str, Any] = {}
    for key in keys:
        value = props.get(key)
        if value in (None, "", [], {}):
            continue
        compact[key] = value
    if not compact:
        return ""
    return json.dumps(compact, ensure_ascii=True, sort_keys=True)


def build_code_text(props: dict[str, Any], labels: list[str]) -> str:
    lines = [
        f"kind: {', '.join(labels)}",
        f"name: {props.get('name', '')}",
        f"path: {props.get('path', '')}",
        f"relative_path: {props.get('relative_path', '')}",
        f"qualified_name: {props.get('qualified_name', props.get('full_name', ''))}",
        f"signature: {props.get('signature', '')}",
        f"line_number: {props.get('line_number', '')}",
        compact_json(props, ["module", "class_name", "parent", "import_path"]),
    ]
    source = str(props.get("source", props.get("source_code", "")))[:800]
    if source:
        lines.append(f"snippet: {source}")
    return "\n".join(line for line in lines if line)


def build_graphiti_text(props: dict[str, Any], labels: list[str]) -> str:
    lines = [
        f"kind: {', '.join(labels)}",
        f"name: {props.get('name', '')}",
        f"summary: {props.get('summary', '')}",
        f"source_description: {props.get('source_description', '')}",
    ]
    content = str(props.get("content", ""))[:2000]
    if content:
        lines.append(f"content: {content}")
    extra = {
        key: value
        for key, value in props.items()
        if key
        not in {
            "name",
            "summary",
            "source_description",
            "content",
            "name_embedding",
            "kg_embedding",
            "created_at",
            "valid_at",
        }
        and value not in (None, "", [], {})
    }
    if extra:
        lines.append(
            f"attributes: {json.dumps(extra, ensure_ascii=True, sort_keys=True)[:1000]}"
        )
    return "\n".join(line for line in lines if line)


def fetch_records(
    client: Neo4jHTTP,
    labels: tuple[str, ...],
    group_id: str | None = None,
    path_prefixes: list[str] | None = None,
) -> list[NodeRecord]:
    label_match = " OR ".join([f"n:{label}" for label in labels])
    where_group = "AND n.group_id = $group_id" if group_id else ""
    where_path = (
        """
        AND (
          (n.path IS NOT NULL AND ANY(prefix IN $path_prefixes WHERE n.path STARTS WITH prefix))
          OR
          (n.relative_path IS NOT NULL AND ANY(prefix IN $path_prefixes WHERE n.relative_path STARTS WITH prefix))
        )
        """
        if path_prefixes
        else ""
    )
    parameters: dict[str, Any] = {}
    if group_id:
        parameters["group_id"] = group_id
    if path_prefixes:
        parameters["path_prefixes"] = path_prefixes
    rows = client.query(
        f"""
        MATCH (n)
        WHERE ({label_match}) {where_group} {where_path}
        RETURN {{
          node_id: id(n),
          labels: labels(n),
          props: properties(n)
        }}
        ORDER BY id(n)
        """,
        parameters,
    )
    records: list[NodeRecord] = []
    for row in rows:
        props = row["props"]
        labels_for_node = row["labels"]
        kind = next(
            (label for label in labels_for_node if label in labels), labels_for_node[0]
        )
        name = str(props.get("name", ""))
        text = (
            build_code_text(props, labels_for_node)
            if kind in CODE_LABELS
            else build_graphiti_text(props, labels_for_node)
        )
        records.append(
            NodeRecord(
                node_id=int(row["node_id"]),
                labels=labels_for_node,
                props=props,
                kind=kind,
                name=name,
                text=text,
                tokens=set(),
                embedding=props.get("kg_embedding")
                if isinstance(props.get("kg_embedding"), list)
                else None,
            )
        )
    return records


def embed_texts(embed_url: str, model_name: str, texts: list[str]) -> list[list[float]]:
    payload = {"model": model_name, "input": texts}
    request = Request(
        embed_url,
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json", "Accept": "application/json"},
        method="POST",
    )
    with urlopen(request) as response:
        data = json.loads(response.read().decode("utf-8"))
    return [embedding[:EMBEDDING_DIM] for embedding in data["embeddings"]]


def cosine_similarity(a: list[float], b: list[float]) -> float:
    if not a or not b:
        return 0.0
    dot = sum(x * y for x, y in zip(a, b, strict=True))
    norm_a = math.sqrt(sum(x * x for x in a))
    norm_b = math.sqrt(sum(y * y for y in b))
    if norm_a == 0 or norm_b == 0:
        return 0.0
    return dot / (norm_a * norm_b)


def batch(rows: list[dict[str, Any]], size: int = 32) -> list[list[dict[str, Any]]]:
    return [rows[index : index + size] for index in range(0, len(rows), size)]


def update_embeddings(
    client: Neo4jHTTP, records: list[NodeRecord], model_name: str
) -> None:
    update_rows = []
    for record in records:
        if not record.embedding:
            continue
        update_rows.append(
            {
                "node_id": record.node_id,
                "embedding": record.embedding,
                "kind": record.kind,
                "model": model_name,
            }
        )
    for chunk in batch(update_rows):
        client.query(
            """
            UNWIND $rows AS row
            MATCH (n)
            WHERE id(n) = row.node_id
            SET n:KGEmbeddingNode,
                n.kg_embedding = row.embedding,
                n.kg_kind = row.kind,
                n.kg_embedding_model = row.model
            RETURN count(n)
            """,
            {"rows": chunk},
        )


def create_vector_index(client: Neo4jHTTP) -> None:
    client.query(
        """
        CREATE VECTOR INDEX kg_embedding_index IF NOT EXISTS
        FOR (n:KGEmbeddingNode)
        ON (n.kg_embedding)
        OPTIONS {
          indexConfig: {
            `vector.dimensions`: 1024,
            `vector.similarity_function`: 'cosine'
          }
        }
        """
    )


def fetch_mentions(client: Neo4jHTTP, group_id: str) -> dict[int, set[int]]:
    rows = client.query(
        """
        MATCH (e:Episodic {group_id: $group_id})-[:MENTIONS]->(n:Entity {group_id: $group_id})
        RETURN {episode_id: id(e), entity_id: id(n)}
        """,
        {"group_id": group_id},
    )
    mentions: dict[int, set[int]] = {}
    for row in rows:
        mentions.setdefault(int(row["entity_id"]), set()).add(int(row["episode_id"]))
    return mentions


def write_links(client: Neo4jHTTP, link_rows: list[dict[str, Any]]) -> None:
    unique_rows = {
        (row["source_id"], row["target_id"], row["strategy"]): row for row in link_rows
    }
    for chunk in batch(list(unique_rows.values())):
        client.query(
            """
            UNWIND $rows AS row
            MATCH (source)
            WHERE id(source) = row.source_id
            MATCH (target)
            WHERE id(target) = row.target_id
            MERGE (source)-[r:REFERS_TO_CODE {strategy: row.strategy}]->(target)
            SET r.score = row.score,
                r.match_text = row.match_text,
                r.source_kind = row.source_kind,
                r.code_kind = row.code_kind
            RETURN count(r)
            """,
            {"rows": chunk},
        )


def clear_links_for_targets(client: Neo4jHTTP, target_ids: list[int]) -> None:
    if not target_ids:
        return
    for chunk in batch([{"target_id": target_id} for target_id in target_ids]):
        client.query(
            """
            UNWIND $rows AS row
            MATCH ()-[r:REFERS_TO_CODE]->(target)
            WHERE id(target) = row.target_id
            DELETE r
            RETURN count(r)
            """,
            {"rows": chunk},
        )


def main() -> int:
    root = repo_root()
    load_dotenv(root / ".env.example")
    load_dotenv(root / ".env")

    embedding_model = os.environ.get("EMBEDDING_MODEL", "qwen3-embedding:4b")
    ensure_embedding_model(embedding_model)
    installed_models = list_ollama_models()

    neo4j_uri = os.environ.get("NEO4J_URI", "bolt://localhost:7687")
    neo4j_http_url = os.environ.get("NEO4J_HTTP_URL", derive_neo4j_http_url(neo4j_uri))
    neo4j_database = os.environ.get("NEO4J_DATABASE", "neo4j")
    neo4j_username = os.environ.get(
        "NEO4J_USERNAME", os.environ.get("NEO4J_USER", "neo4j")
    )
    neo4j_password = os.environ.get("NEO4J_PASSWORD")
    if not neo4j_password:
        raise RuntimeError("NEO4J_PASSWORD must be set.")

    graphiti_group_id = os.environ.get("GRAPHITI_GROUP_ID", "litkg-docs")
    code_path_prefixes = scoped_path_prefixes(
        os.environ.get("KG_CODE_PATH_PREFIX"), root
    )
    ollama_base_url = os.environ.get("OLLAMA_BASE_URL", "http://localhost:11434/v1")
    embed_url = ollama_embed_url(ollama_base_url)
    chat_url = ollama_chat_url(ollama_base_url)
    client = Neo4jHTTP(neo4j_http_url, neo4j_database, neo4j_username, neo4j_password)

    code_records = fetch_records(client, CODE_LABELS, path_prefixes=code_path_prefixes)
    graphiti_records = fetch_records(
        client, GRAPHITI_LABELS, group_id=graphiti_group_id
    )
    if code_path_prefixes and not code_records:
        print(
            f"No matching code nodes found for KG_CODE_PATH_PREFIX={os.environ.get('KG_CODE_PATH_PREFIX')}.",
            file=sys.stderr,
        )
        return 1
    if not code_records and not graphiti_records:
        print(
            "No matching Neo4j nodes found for embedding enrichment.", file=sys.stderr
        )
        return 1

    ignored_tokens = infer_common_tokens(
        chat_url,
        pick_token_filter_model(installed_models),
        code_records,
        graphiti_records,
    )

    all_records = code_records + graphiti_records
    for record in all_records:
        record.tokens = normalize_tokens(record.text, ignored_tokens)
    embed_limit = int(os.environ.get("KG_EMBED_LIMIT", "0"))
    if embed_limit > 0:
        all_records = all_records[:embed_limit]
    record_by_id = {record.node_id: record for record in all_records}
    records_to_embed = [record for record in all_records if record.embedding is None]
    for record_batch in batch(
        [
            {"node_id": record.node_id, "text": record.text}
            for record in records_to_embed
        ],
        size=32,
    ):
        embeddings = embed_texts(
            embed_url,
            embedding_model,
            [row["text"] for row in record_batch],
        )
        for row, embedding in zip(record_batch, embeddings, strict=True):
            record_by_id[row["node_id"]].embedding = embedding

    update_embeddings(client, records_to_embed, embedding_model)
    create_vector_index(client)

    mentions_by_entity = fetch_mentions(client, graphiti_group_id)
    code_name_index: dict[str, list[NodeRecord]] = {}
    for record in code_records:
        if record.name:
            code_name_index.setdefault(record.name.lower(), []).append(record)

    episode_records = [
        record for record in graphiti_records if record.kind == "Episodic"
    ]
    entity_records = [record for record in graphiti_records if record.kind == "Entity"]

    episode_path_links: dict[int, set[int]] = {}
    link_rows: list[dict[str, Any]] = []

    for record in episode_records:
        for path_ref in PATH_REF_RE.findall(record.text):
            normalized_ref = path_ref.lstrip("./")
            for code_record in code_records:
                code_path = str(code_record.props.get("path", ""))
                relative_path = str(code_record.props.get("relative_path", ""))
                if (
                    code_path.endswith(normalized_ref)
                    or relative_path == normalized_ref
                ):
                    episode_path_links.setdefault(record.node_id, set()).add(
                        code_record.node_id
                    )
                    link_rows.append(
                        {
                            "source_id": record.node_id,
                            "target_id": code_record.node_id,
                            "strategy": "path",
                            "score": 1.0,
                            "match_text": normalized_ref,
                            "source_kind": record.kind,
                            "code_kind": code_record.kind,
                        }
                    )

        for code_name, candidates in code_name_index.items():
            if len(candidates) != 1 or len(code_name) < 6:
                continue
            if re.search(rf"\b{re.escape(code_name)}\b", record.text.lower()):
                target = candidates[0]
                link_rows.append(
                    {
                        "source_id": record.node_id,
                        "target_id": target.node_id,
                        "strategy": "exact_symbol",
                        "score": 0.95,
                        "match_text": code_name,
                        "source_kind": record.kind,
                        "code_kind": target.kind,
                    }
                )

    for record in entity_records:
        exact_candidates = code_name_index.get(record.name.lower(), [])
        if record.name and len(record.name) >= 4 and len(exact_candidates) <= 3:
            for target in exact_candidates:
                link_rows.append(
                    {
                        "source_id": record.node_id,
                        "target_id": target.node_id,
                        "strategy": "exact_symbol",
                        "score": 0.98,
                        "match_text": record.name,
                        "source_kind": record.kind,
                        "code_kind": target.kind,
                    }
                )

        for episode_id in mentions_by_entity.get(record.node_id, set()):
            for target_id in episode_path_links.get(episode_id, set()):
                target = next(
                    code_record
                    for code_record in code_records
                    if code_record.node_id == target_id
                )
                link_rows.append(
                    {
                        "source_id": record.node_id,
                        "target_id": target_id,
                        "strategy": "path",
                        "score": 0.9,
                        "match_text": target.name or str(target.props.get("path", "")),
                        "source_kind": record.kind,
                        "code_kind": target.kind,
                    }
                )

    for record in episode_records + entity_records:
        existing_targets = {
            row["target_id"] for row in link_rows if row["source_id"] == record.node_id
        }
        hinted_candidates = [
            code_record
            for code_record in code_records
            if code_record.node_id not in existing_targets
            and len(record.tokens & code_record.tokens) >= 2
        ]
        hinted_candidates.sort(
            key=lambda code_record: (
                len(record.tokens & code_record.tokens),
                code_record.kind == "Module",
                code_record.kind == "File",
            ),
            reverse=True,
        )
        for target in hinted_candidates[:2]:
            link_rows.append(
                {
                    "source_id": record.node_id,
                    "target_id": target.node_id,
                    "strategy": "package_hint",
                    "score": float(len(record.tokens & target.tokens)),
                    "match_text": ", ".join(sorted(record.tokens & target.tokens)[:5]),
                    "source_kind": record.kind,
                    "code_kind": target.kind,
                }
            )

        existing_targets = {
            row["target_id"] for row in link_rows if row["source_id"] == record.node_id
        }
        embedding_candidates = []
        for code_record in code_records:
            if code_record.node_id in existing_targets:
                continue
            shared_tokens = record.tokens & code_record.tokens
            if not shared_tokens:
                continue
            similarity = cosine_similarity(
                record.embedding or [], code_record.embedding or []
            )
            if similarity < 0.80:
                continue
            embedding_candidates.append((similarity, shared_tokens, code_record))
        embedding_candidates.sort(key=lambda item: item[0], reverse=True)
        if embedding_candidates:
            similarity, shared_tokens, target = embedding_candidates[0]
            link_rows.append(
                {
                    "source_id": record.node_id,
                    "target_id": target.node_id,
                    "strategy": "embedding",
                    "score": round(similarity, 6),
                    "match_text": ", ".join(sorted(shared_tokens)[:5]),
                    "source_kind": record.kind,
                    "code_kind": target.kind,
                }
            )

    clear_links_for_targets(client, [record.node_id for record in code_records])
    write_links(client, link_rows)
    scope_suffix = ""
    if code_path_prefixes:
        scope_suffix = f" for path {os.environ.get('KG_CODE_PATH_PREFIX')}"
    print(
        f"Embedded {len(records_to_embed)} nodes, refreshed links to {len(code_records)} code nodes{scope_suffix}, and wrote {len(link_rows)} REFERS_TO_CODE candidates."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
