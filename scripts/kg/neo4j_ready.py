#!/usr/bin/env python3
"""Fail-fast Neo4j readiness check for litkg runtime KG scripts."""

from __future__ import annotations

import argparse
import base64
import json
import os
import sys
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import Request, urlopen


def load_dotenv(dotenv_path: str) -> None:
    if not os.path.exists(dotenv_path):
        return
    with open(dotenv_path, encoding="utf-8") as handle:
        for raw_line in handle:
            line = raw_line.strip()
            if not line or line.startswith("#") or "=" not in line:
                continue
            key, value = line.split("=", 1)
            os.environ.setdefault(key.strip(), value.strip().strip('"').strip("'"))


def derive_http_url(neo4j_uri: str) -> str:
    parsed = urlparse(neo4j_uri)
    host = parsed.hostname or "localhost"
    return f"http://{host}:7474"


def check_neo4j(base_url: str, database: str, username: str, password: str) -> None:
    auth = base64.b64encode(f"{username}:{password}".encode("utf-8")).decode("ascii")
    request = Request(
        f"{base_url.rstrip('/')}/db/{database}/tx/commit",
        data=json.dumps(
            {
                "statements": [
                    {
                        "statement": "RETURN 1 AS ok",
                        "resultDataContents": ["row"],
                    }
                ]
            }
        ).encode("utf-8"),
        headers={
            "Authorization": f"Basic {auth}",
            "Content-Type": "application/json",
            "Accept": "application/json",
        },
        method="POST",
    )
    with urlopen(request, timeout=5.0) as response:
        body = json.loads(response.read().decode("utf-8"))
    errors = body.get("errors", [])
    if errors:
        raise RuntimeError(errors[0])


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", default=None)
    args = parser.parse_args()

    if args.repo_root:
        load_dotenv(os.path.join(args.repo_root, ".env.example"))
        load_dotenv(os.path.join(args.repo_root, ".env"))

    neo4j_uri = os.environ.get("NEO4J_URI", "bolt://localhost:7687")
    neo4j_http_url = os.environ.get("NEO4J_HTTP_URL", derive_http_url(neo4j_uri))
    neo4j_database = os.environ.get("NEO4J_DATABASE", "neo4j")
    neo4j_username = os.environ.get("NEO4J_USERNAME", os.environ.get("NEO4J_USER", "neo4j"))
    neo4j_password = os.environ.get("NEO4J_PASSWORD", "litkglocal")

    try:
        check_neo4j(neo4j_http_url, neo4j_database, neo4j_username, neo4j_password)
    except (HTTPError, URLError, TimeoutError, OSError, RuntimeError) as exc:
        print(
            f"[error] Neo4j is not reachable at {neo4j_http_url} for database "
            f"{neo4j_database}: {exc}\n"
            "Start the litkg Neo4j runtime first with `make kg-up`, then rerun "
            "the KG command.",
            file=sys.stderr,
        )
        return 1

    print(f"[ok] Neo4j reachable at {neo4j_http_url}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
