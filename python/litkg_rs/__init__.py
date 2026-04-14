"""Python-facing API for litkg-rs tabular exports."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from . import _native


def load_parsed_papers(parsed_root: str | Path) -> list[dict[str, Any]]:
    raw = _native.load_parsed_papers_json(str(parsed_root))
    return json.loads(raw)


def build_tabular_bundle(parsed_root: str | Path) -> dict[str, list[dict[str, Any]]]:
    raw = _native.build_tabular_bundle_json(str(parsed_root))
    return json.loads(raw)


def build_tabular_bundle_with_notebooks(
    parsed_root: str | Path, notebook_root: str | Path
) -> dict[str, list[dict[str, Any]]]:
    raw = _native.build_tabular_bundle_with_notebooks_json(
        str(parsed_root), str(notebook_root)
    )
    return json.loads(raw)


def load_notebooks(notebook_root: str | Path) -> list[dict[str, Any]]:
    raw = _native.load_notebooks_json(str(notebook_root))
    return json.loads(raw)


def write_tabular_exports(parsed_root: str | Path, output_root: str | Path) -> None:
    _native.write_tabular_exports_from_parsed(str(parsed_root), str(output_root))


def write_tabular_exports_with_notebooks(
    parsed_root: str | Path, notebook_root: str | Path, output_root: str | Path
) -> None:
    _native.write_tabular_exports_from_parsed_with_notebooks(
        str(parsed_root), str(notebook_root), str(output_root)
    )


def papers_dataframe(parsed_root: str | Path):
    bundle = build_tabular_bundle(parsed_root)
    try:
        import pandas as pd
    except ImportError as error:
        raise RuntimeError("pandas is required for papers_dataframe()") from error
    return pd.DataFrame(bundle["papers"])


__all__ = [
    "build_tabular_bundle",
    "build_tabular_bundle_with_notebooks",
    "load_parsed_papers",
    "load_notebooks",
    "papers_dataframe",
    "write_tabular_exports",
    "write_tabular_exports_with_notebooks",
]
