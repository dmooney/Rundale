"""Tiny GitHub REST wrapper used by collectors.

We don't have `gh` in the remote execution environment and the parent
Claude session uses the GitHub MCP server, but the workflow that drives
this loop runs in Actions and has `GITHUB_TOKEN` in env. Using `urllib`
keeps the script's dependency surface to just `anthropic`.
"""

from __future__ import annotations

import json
import os
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Any, Iterator, Optional

DEFAULT_REPO = os.environ.get("GITHUB_REPOSITORY", "dmooney/rundale")
API = "https://api.github.com"


class GitHubError(RuntimeError):
    pass


def _token() -> Optional[str]:
    return os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")


def _request(
    path: str,
    *,
    params: Optional[dict[str, Any]] = None,
    accept: str = "application/vnd.github+json",
) -> Any:
    url = f"{API}{path}"
    if params:
        url = f"{url}?{urllib.parse.urlencode(params)}"
    req = urllib.request.Request(url)
    req.add_header("Accept", accept)
    req.add_header("X-GitHub-Api-Version", "2022-11-28")
    token = _token()
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    for attempt in range(4):
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                if resp.status == 204:
                    return None
                body = resp.read()
                if accept.startswith("application/vnd.github+json"):
                    return json.loads(body)
                return body
        except urllib.error.HTTPError as exc:
            if exc.code == 404:
                return None
            if exc.code in (403, 502, 503, 504) and attempt < 3:
                time.sleep(2 ** attempt)
                continue
            raise GitHubError(f"HTTP {exc.code} for {url}: {exc.read()[:400]!r}") from exc
        except urllib.error.URLError as exc:
            if attempt < 3:
                time.sleep(2 ** attempt)
                continue
            raise GitHubError(f"network error for {url}: {exc}") from exc
    raise GitHubError(f"exhausted retries for {url}")


def get(path: str, **params: Any) -> Any:
    return _request(path, params=params or None)


def paginate(path: str, *, per_page: int = 50, max_pages: int = 6, **params: Any) -> Iterator[Any]:
    """Yield pages until exhausted or `max_pages` reached."""
    page = 1
    while page <= max_pages:
        params_with_page = dict(params)
        params_with_page["per_page"] = per_page
        params_with_page["page"] = page
        result = _request(path, params=params_with_page)
        if not result:
            return
        if not isinstance(result, list):
            yield result
            return
        yield from result
        if len(result) < per_page:
            return
        page += 1


def fetch_log_text(path: str, max_bytes: int = 200_000) -> str:
    """Download a workflow-run log (zip of text files) and return the
    concatenated text, capped at `max_bytes` for prompt budget."""
    import io
    import zipfile

    raw = _request(path, accept="application/zip")
    if raw is None:
        return ""
    buf = io.BytesIO(raw)
    chunks: list[str] = []
    total = 0
    try:
        with zipfile.ZipFile(buf) as zf:
            for name in zf.namelist():
                with zf.open(name) as fp:
                    text = fp.read().decode("utf-8", errors="replace")
                snippet = f"\n----- {name} -----\n{text}\n"
                if total + len(snippet) > max_bytes:
                    chunks.append(snippet[: max_bytes - total])
                    break
                chunks.append(snippet)
                total += len(snippet)
    except zipfile.BadZipFile:
        return ""
    return "".join(chunks)
