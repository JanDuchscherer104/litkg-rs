#!/usr/bin/env python3
"""HTTP preflight helpers for Ollama-backed litkg KG workflows."""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
import tomllib
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

DEFAULT_BASE_URL = "http://localhost:11434/v1"
DEFAULT_EMBEDDING_MODEL = "qwen3-embedding:4b"
DEFAULT_CHAT_MODEL = "gemma4:26b"
DEFAULT_EMBEDDING_DIM = 2560
DEFAULT_TIMEOUT_S = 120.0


class OllamaError(RuntimeError):
    """Raised when the configured Ollama endpoint is unavailable or unsuitable."""


@dataclass(frozen=True)
class OllamaSettings:
    base_url: str
    embedding_model: str
    embedding_dim: int
    chat_model: str


def read_config(config_path: str | None) -> dict[str, Any]:
    if not config_path:
        return {}
    path = Path(config_path).expanduser()
    if not path.exists():
        raise OllamaError(f"Ollama config file does not exist: {config_path}")
    try:
        with path.open("rb") as handle:
            data = tomllib.load(handle)
    except tomllib.TOMLDecodeError as exc:
        raise OllamaError(f"Could not parse TOML config {config_path}: {exc}") from exc
    if not isinstance(data, dict):
        return {}
    return data


def config_ollama_table(config_path: str | None) -> dict[str, Any]:
    data = read_config(config_path)
    runtime = data.get("runtime", {})
    if isinstance(runtime, dict) and isinstance(runtime.get("ollama"), dict):
        return runtime["ollama"]
    ollama = data.get("ollama", {})
    if isinstance(ollama, dict):
        return ollama
    return {}


def resolve_settings(
    *,
    config_path: str | None = None,
    base_url: str | None = None,
    embedding_model: str | None = None,
    embedding_dim: int | None = None,
    chat_model: str | None = None,
    use_env: bool = True,
) -> OllamaSettings:
    table = config_ollama_table(config_path)

    def pick_string(cli_value: str | None, key: str, env_key: str, default: str) -> str:
        if cli_value:
            return cli_value
        config_value = table.get(key)
        if isinstance(config_value, str) and config_value.strip():
            return config_value.strip()
        if use_env:
            env_value = os.environ.get(env_key)
            if env_value and env_value.strip():
                return env_value.strip()
        return default

    def pick_int(cli_value: int | None, key: str, env_key: str, default: int) -> int:
        if cli_value is not None:
            return cli_value
        config_value = table.get(key)
        if isinstance(config_value, int):
            return config_value
        if isinstance(config_value, str) and config_value.strip():
            return int(config_value.strip())
        if use_env:
            env_value = os.environ.get(env_key)
            if env_value and env_value.strip():
                return int(env_value.strip())
        return default

    return OllamaSettings(
        base_url=pick_string(base_url, "base_url", "OLLAMA_BASE_URL", DEFAULT_BASE_URL),
        embedding_model=pick_string(
            embedding_model,
            "embedding_model",
            "EMBEDDING_MODEL",
            DEFAULT_EMBEDDING_MODEL,
        ),
        embedding_dim=pick_int(
            embedding_dim,
            "embedding_dim",
            "EMBEDDING_DIM",
            DEFAULT_EMBEDDING_DIM,
        ),
        chat_model=pick_string(
            chat_model,
            "chat_model",
            "GRAPHITI_LLM_MODEL",
            DEFAULT_CHAT_MODEL,
        ),
    )


def api_base_url(base_url: str | None = None) -> str:
    """Return the Ollama native API base URL from either native or /v1 input."""
    raw_url = (base_url or os.environ.get("OLLAMA_BASE_URL") or DEFAULT_BASE_URL).strip()
    if not raw_url:
        raw_url = DEFAULT_BASE_URL
    trimmed = raw_url.rstrip("/")
    if trimmed.endswith("/v1"):
        trimmed = trimmed[:-3]
    return trimmed.rstrip("/")


def openai_base_url(base_url: str | None = None) -> str:
    """Return the OpenAI-compatible Ollama base URL used by Graphiti clients."""
    return f"{api_base_url(base_url)}/v1"


def endpoint_url(base_url: str | None, path: str) -> str:
    return f"{api_base_url(base_url)}/{path.lstrip('/')}"


def embed_url(base_url: str | None = None) -> str:
    return endpoint_url(base_url, "/api/embed")


def chat_url(base_url: str | None = None) -> str:
    return endpoint_url(base_url, "/api/chat")


def request_json(
    base_url: str | None,
    path: str,
    *,
    payload: dict[str, Any] | None = None,
    timeout_s: float = DEFAULT_TIMEOUT_S,
) -> dict[str, Any]:
    data = json.dumps(payload).encode("utf-8") if payload is not None else None
    request = Request(
        endpoint_url(base_url, path),
        data=data,
        headers={"Content-Type": "application/json", "Accept": "application/json"},
        method="POST" if payload is not None else "GET",
    )
    try:
        with urlopen(request, timeout=timeout_s) as response:
            decoded = json.loads(response.read().decode("utf-8"))
    except (HTTPError, URLError, TimeoutError, json.JSONDecodeError) as exc:
        raise OllamaError(
            f"Could not reach Ollama at {api_base_url(base_url)}. "
            "Start Ollama on the model host and expose it to this machine, for "
            "example with: ssh -R 11434:127.0.0.1:11434 <ubuntu-host>"
        ) from exc
    if not isinstance(decoded, dict):
        raise OllamaError(f"Ollama returned a non-object JSON response from {path}.")
    return decoded


def parse_model_names(tags_payload: dict[str, Any]) -> list[str]:
    models = tags_payload.get("models", [])
    if not isinstance(models, list):
        raise OllamaError("Ollama /api/tags response did not contain a models list.")
    names: list[str] = []
    for model in models:
        if not isinstance(model, dict):
            continue
        name = model.get("name") or model.get("model")
        if isinstance(name, str) and name.strip():
            names.append(name.strip())
    return sorted(set(names))


def list_models(base_url: str | None = None) -> list[str]:
    return parse_model_names(request_json(base_url, "/api/tags"))


def missing_models(installed_models: list[str], required_models: list[str]) -> list[str]:
    installed = set(installed_models)
    return [model for model in required_models if model not in installed]


def require_models(
    base_url: str | None,
    required_models: list[str],
    *,
    installed_models: list[str] | None = None,
) -> list[str]:
    installed = installed_models if installed_models is not None else list_models(base_url)
    missing = missing_models(installed, required_models)
    if missing:
        raise OllamaError(
            "Ollama is reachable, but required model(s) are missing: "
            f"{', '.join(missing)}. Install them on the model host with "
            f"`ollama pull {' && ollama pull '.join(missing)}`."
        )
    return installed


def extract_embedding_dim(embed_payload: dict[str, Any]) -> int:
    embeddings = embed_payload.get("embeddings")
    if not isinstance(embeddings, list) or not embeddings:
        raise OllamaError("Ollama /api/embed response did not include embeddings.")
    first_embedding = embeddings[0]
    if not isinstance(first_embedding, list):
        raise OllamaError("Ollama /api/embed returned a malformed embedding.")
    return len(first_embedding)


def probe_embedding_dim(
    base_url: str | None,
    model: str,
    *,
    expected_dim: int,
    timeout_s: float = DEFAULT_TIMEOUT_S,
) -> int:
    payload = request_json(
        base_url,
        "/api/embed",
        payload={"model": model, "input": "litkg ollama preflight"},
        timeout_s=timeout_s,
    )
    dim = extract_embedding_dim(payload)
    if dim != expected_dim:
        raise OllamaError(
            f"Embedding model {model} returned dimension {dim}, expected {expected_dim}."
        )
    return dim


def probe_chat(
    base_url: str | None,
    model: str,
    *,
    timeout_s: float = DEFAULT_TIMEOUT_S,
) -> str:
    payload = request_json(
        base_url,
        "/api/chat",
        payload={
            "model": model,
            "stream": False,
            "options": {"temperature": 0, "seed": 0},
            "messages": [
                {"role": "system", "content": "Return a short plain response."},
                {"role": "user", "content": "litkg preflight"},
            ],
        },
        timeout_s=timeout_s,
    )
    message = payload.get("message", {})
    content = message.get("content") if isinstance(message, dict) else None
    if not isinstance(content, str) or not content.strip():
        raise OllamaError(f"Chat model {model} returned an empty response.")
    return content.strip()


def run_check(args: argparse.Namespace) -> int:
    settings = resolve_settings(
        config_path=args.config,
        base_url=args.base_url,
        embedding_model=args.embedding_model,
        embedding_dim=args.embedding_dim,
        chat_model=args.chat_model,
    )
    required_models = [settings.embedding_model, settings.chat_model]
    installed_models = require_models(settings.base_url, required_models)
    dim = probe_embedding_dim(
        settings.base_url,
        settings.embedding_model,
        expected_dim=settings.embedding_dim,
        timeout_s=args.timeout_s,
    )
    chat_preview = probe_chat(settings.base_url, settings.chat_model, timeout_s=args.timeout_s)
    if args.format == "json":
        print(
            json.dumps(
                {
                    "base_url": api_base_url(settings.base_url),
                    "embedding_model": settings.embedding_model,
                    "embedding_dim": dim,
                    "chat_model": settings.chat_model,
                    "chat_response_preview": chat_preview[:80],
                    "installed_model_count": len(installed_models),
                },
                sort_keys=True,
            )
        )
    else:
        print(f"[ok] Ollama reachable at {api_base_url(settings.base_url)}")
        print(f"[ok] embedding model {settings.embedding_model} returned {dim} dimensions")
        print(f"[ok] chat model {settings.chat_model} returned a response")
    return 0


def run_env(args: argparse.Namespace) -> int:
    settings = resolve_settings(
        config_path=args.config,
        base_url=args.base_url,
        embedding_model=args.embedding_model,
        embedding_dim=args.embedding_dim,
        chat_model=args.chat_model,
    )
    exports = {
        "OLLAMA_BASE_URL": openai_base_url(settings.base_url),
        "EMBEDDING_MODEL": settings.embedding_model,
        "EMBEDDING_DIM": str(settings.embedding_dim),
        "GRAPHITI_LLM_MODEL": settings.chat_model,
    }
    for key, value in exports.items():
        print(f"export {key}={json.dumps(value)}")
    return 0


def run_self_test() -> int:
    assert api_base_url("http://localhost:11434/v1") == "http://localhost:11434"
    assert api_base_url("http://localhost:11434") == "http://localhost:11434"
    assert openai_base_url("http://localhost:11434") == "http://localhost:11434/v1"
    assert openai_base_url("http://localhost:11434/v1") == "http://localhost:11434/v1"
    assert embed_url("http://localhost:11434/v1") == "http://localhost:11434/api/embed"
    assert chat_url("http://localhost:11434") == "http://localhost:11434/api/chat"
    tags = {"models": [{"name": "qwen3-embedding:4b"}, {"model": "gemma4:26b"}]}
    installed = parse_model_names(tags)
    assert missing_models(installed, ["qwen3-embedding:4b", "gemma4:26b"]) == []
    assert missing_models(installed, ["missing:model"]) == ["missing:model"]
    assert extract_embedding_dim({"embeddings": [[0.0] * 2560]}) == 2560
    with tempfile.NamedTemporaryFile("w", suffix=".toml") as handle:
        handle.write(
            "[runtime.ollama]\n"
            'base_url = "http://127.0.0.1:11434/v1"\n'
            'embedding_model = "qwen3-embedding:4b"\n'
            "embedding_dim = 2560\n"
            'chat_model = "gemma4:26b"\n'
        )
        handle.flush()
        settings = resolve_settings(config_path=handle.name)
        assert settings.embedding_model == "qwen3-embedding:4b"
        assert settings.embedding_dim == 2560
        assert settings.chat_model == "gemma4:26b"
    print("[ok] ollama_http self-test passed")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    check = subparsers.add_parser("check", help="Validate the configured Ollama endpoint.")
    check.add_argument("--config", default=None)
    check.add_argument("--base-url", default=None)
    check.add_argument(
        "--embedding-model",
        default=None,
    )
    check.add_argument(
        "--chat-model",
        default=None,
    )
    check.add_argument(
        "--embedding-dim",
        type=int,
        default=None,
    )
    check.add_argument("--timeout-s", type=float, default=DEFAULT_TIMEOUT_S)
    check.add_argument("--format", choices=("text", "json"), default="text")
    check.set_defaults(func=run_check)

    self_test = subparsers.add_parser("self-test", help="Run local parser tests.")
    self_test.set_defaults(func=lambda _args: run_self_test())

    openai_url = subparsers.add_parser(
        "openai-url", help="Print a normalized OpenAI-compatible Ollama URL."
    )
    openai_url.add_argument("--config", default=None)
    openai_url.add_argument("--base-url", default=None)
    openai_url.set_defaults(
        func=lambda args: print(
            openai_base_url(resolve_settings(config_path=args.config, base_url=args.base_url).base_url)
        )
        or 0
    )

    env = subparsers.add_parser("env", help="Print shell exports for resolved Ollama settings.")
    env.add_argument("--config", default=None)
    env.add_argument("--base-url", default=None)
    env.add_argument("--embedding-model", default=None)
    env.add_argument("--chat-model", default=None)
    env.add_argument("--embedding-dim", type=int, default=None)
    env.set_defaults(func=run_env)
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except OllamaError as exc:
        print(f"[error] {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
