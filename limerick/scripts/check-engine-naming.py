#!/usr/bin/env python3
"""Reject accidental reintroduction of the old engine identity.

The checker deliberately has no dependency on repository layout beyond Git and
the exception file.  It scans path names, symlink targets, and textual file
contents.  Binary/invalid UTF-8 files are opaque and are reported as skipped.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

TOKEN = "pa" + "rish"
SURFACES = {"path", "content", "symlink"}
CATEGORIES = {"geographic-vocabulary", "scanner-test", "immutable-provenance"}
REQUIRED = {"path", "surface", "literal", "category", "reason"}
ENTRY_KEYS = REQUIRED | {"context", "sha256"}


def display(root: Path, path: Path) -> str:
    try:
        return path.relative_to(root).as_posix()
    except ValueError:
        return path.as_posix()


def safe_relative(value: object) -> Path | None:
    if not isinstance(value, str) or not value or "\\" in value:
        return None
    p = Path(value)
    if p.is_absolute() or ".." in p.parts or "." in p.parts:
        return None
    return p


def safe_target(root: Path, path: Path) -> bool:
    """Allow reads only when no path component (including parents) is a symlink."""
    try:
        relative = path.relative_to(root)
    except ValueError:
        return False
    current = root
    for index, component in enumerate(relative.parts):
        current /= component
        if current.is_symlink() and index != len(relative.parts) - 1:
            return False
    return True


def git_paths(root: Path) -> list[Path]:
    result = subprocess.run(
        ["git", "-C", str(root), "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        check=True,
        stdout=subprocess.PIPE,
    )
    paths = []
    for raw in result.stdout.split(b"\0"):
        if raw:
            paths.append(root / os.fsdecode(raw))
    return paths


def generated_paths(root: Path, roots: list[str]) -> list[Path]:
    found: list[Path] = []
    for raw in roots:
        candidate = Path(raw)
        if not candidate.is_absolute():
            candidate = root / candidate
        candidate = candidate.resolve()
        try:
            candidate.relative_to(root.resolve())
        except ValueError as exc:
            raise ValueError(f"generated root outside --root: {candidate}") from exc
        if not candidate.exists():
            raise ValueError(f"generated root does not exist: {candidate}")
        if not candidate.is_dir():
            raise ValueError(f"generated root is not a directory: {candidate}")
        for directory, dirnames, filenames in os.walk(candidate, followlinks=False):
            symlink_dirs = [name for name in dirnames if (Path(directory) / name).is_symlink()]
            found.extend(Path(directory) / name for name in symlink_dirs)
            dirnames[:] = [name for name in dirnames if name not in symlink_dirs]
            found.extend(Path(directory) / name for name in filenames)
    return found


def read_text(path: Path) -> str | None:
    try:
        data = path.read_bytes()
    except OSError:
        return None
    if b"\0" in data:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return None


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_manifest(path: Path) -> list[dict]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError(f"cannot read exceptions manifest: {exc}") from exc
    if not isinstance(payload, dict) or set(payload) != {"exceptions"}:
        raise ValueError("exceptions manifest has unknown root fields")
    entries = payload.get("exceptions")
    if not isinstance(entries, list):
        raise ValueError("exceptions manifest root must contain an exceptions list")
    return entries


def validate_manifest(
    root: Path,
    manifest: Path,
    entries: list[dict],
    token: str,
    generated: set[str] | None = None,
) -> list[str]:
    errors: list[str] = []
    seen: set[tuple] = set()
    for index, entry in enumerate(entries):
        label = f"manifest entry {index + 1}"
        if not isinstance(entry, dict) or not REQUIRED.issubset(entry):
            errors.append(f"{label}: missing required fields")
            continue
        if set(entry) - ENTRY_KEYS:
            errors.append(f"{label}: unknown fields")
            continue
        rel = safe_relative(entry.get("path"))
        surface, literal, context, category = (
            entry.get(k) for k in ("surface", "literal", "context", "category")
        )
        if rel is None or surface not in SURFACES or not isinstance(literal, str) or not literal:
            errors.append(f"{label}: invalid path/surface/literal")
            continue
        if context is not None and not isinstance(context, str):
            errors.append(f"{label}: context and reason must be non-empty strings")
            continue
        if not isinstance(entry.get("reason"), str) or not entry["reason"]:
            errors.append(f"{label}: reason must be a non-empty string")
            continue
        if context is None and not (
            surface == "content" and (category == "immutable-provenance" or generated is not None)
        ):
            errors.append(f"{label}: context is required except for immutable content")
            continue
        if context == "" and category != "immutable-provenance":
            errors.append(f"{label}: context must be a complete non-empty line")
            continue
        if literal.casefold() != token.casefold():
            errors.append(f"{label}: literal must be the forbidden identity")
            continue
        if category not in CATEGORIES:
            errors.append(f"{label}: unsupported category")
        if generated is not None:
            if surface not in {"content", "path"}:
                errors.append(f"{label}: unsupported generated surface")
            if category not in {"geographic-vocabulary", "immutable-provenance"}:
                errors.append(f"{label}: unsupported generated category")
            if rel.as_posix() not in generated:
                errors.append(f"{label}: path is outside enumerated generated files")
            if context is not None and context == "":
                errors.append(f"{label}: context must be a complete non-empty line")
            expected_hash = entry.get("sha256")
            target_for_hash = root / rel
            if (
                not isinstance(expected_hash, str)
                or len(expected_hash) != 64
                or not target_for_hash.is_file()
                or target_for_hash.is_symlink()
                or expected_hash != sha256(target_for_hash)
            ):
                errors.append(f"{label}: generated content hash mismatch")
        key = (rel.as_posix(), surface, literal, context)
        if key in seen:
            errors.append(f"{label}: duplicate exception")
        seen.add(key)
        target = root / rel
        if rel.as_posix() == display(root, manifest):
            errors.append(f"{rel.as_posix()}: manifest cannot be an exception target")
            continue
        if not target.exists() and not target.is_symlink():
            errors.append(f"{display(root, target)}: stale exception path")
            continue
        if not safe_target(root, target):
            errors.append(f"{display(root, target)}: exception target follows a symlink")
            continue
        if surface == "path":
            if (
                token.casefold() not in target.name.casefold()
                and token.casefold() not in rel.as_posix().casefold()
            ):
                errors.append(f"{display(root, target)}: stale path exception")
            if context != rel.as_posix() or literal.casefold() not in rel.as_posix().casefold():
                errors.append(f"{display(root, target)}: path exception context mismatch")
        elif surface == "symlink":
            if not target.is_symlink():
                errors.append(f"{display(root, target)}: stale symlink exception")
            else:
                current = os.readlink(target)
                if current != context or literal.casefold() not in current.casefold():
                    errors.append(f"{display(root, target)}: symlink exception context mismatch")
        else:
            text = read_text(target)
            if text is None:
                errors.append(f"{display(root, target)}: content exception is opaque or unreadable")
            else:
                matching = [
                    line for line in text.splitlines() if literal.casefold() in line.casefold()
                ]
                if not matching:
                    errors.append(
                        f"{display(root, target)}: stale content exception (no candidate)"
                    )
                if context is not None and context not in matching:
                    errors.append(f"{display(root, target)}: stale content exception context")
                if category == "immutable-provenance":
                    expected = entry.get("sha256")
                    if (
                        not isinstance(expected, str)
                        or len(expected) != 64
                        or expected != sha256(target)
                    ):
                        errors.append(f"{display(root, target)}: immutable content hash mismatch")
    return errors


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--exceptions", type=Path)
    parser.add_argument("--generated-root", action="append", default=[])
    parser.add_argument("--generated-exceptions", type=Path)
    args = parser.parse_args(argv)
    root = args.root.resolve()
    manifest = (
        args.exceptions or root / "limerick/scripts/engine-naming-exceptions.json"
    ).resolve()
    token_cf = TOKEN.casefold()
    try:
        if args.generated_exceptions and not args.generated_root:
            raise ValueError("--generated-exceptions requires at least one --generated-root")
        entries = load_manifest(manifest)
        generated_paths_list = generated_paths(root, args.generated_root)
        errors = validate_manifest(root, manifest, entries, TOKEN)
        generated_entries: list[dict] = []
        generated_manifest = None
        if args.generated_exceptions:
            generated_manifest = args.generated_exceptions.resolve()
            generated_entries = load_manifest(generated_manifest)
            errors.extend(
                validate_manifest(
                    root,
                    generated_manifest,
                    generated_entries,
                    TOKEN,
                    {display(root, path) for path in generated_paths_list},
                )
            )
        all_entries = entries + generated_entries
        allowed = {
            (e.get("path"), e.get("surface"), e.get("literal", "").casefold(), e.get("context"))
            for e in all_entries
            if isinstance(e, dict) and e.get("literal", "").casefold() == token_cf
        }
        immutable: set[tuple[str, str, str]] = set()
        for entry in all_entries:
            if (
                not isinstance(entry, dict)
                or entry.get("surface") != "content"
                or entry.get("context") is not None
                or (
                    entry.get("category") != "immutable-provenance"
                    and entry not in generated_entries
                )
            ):
                continue
            rel_entry = safe_relative(entry.get("path"))
            if rel_entry is not None and isinstance(entry.get("sha256"), str):
                target_entry = root / rel_entry
                if target_entry.is_file() and entry["sha256"] == sha256(target_entry):
                    immutable.add(
                        (rel_entry.as_posix(), "content", entry.get("literal", "").casefold())
                    )
        paths = git_paths(root)
        paths.extend(generated_paths_list)
    except (OSError, subprocess.CalledProcessError, ValueError) as exc:
        print(f"naming guard: {exc}", file=sys.stderr)
        return 2
    unique = {p for p in paths if (p.exists() or p.is_symlink()) and safe_target(root, p)}
    manifest_display = display(root, manifest)
    generated_manifest_display = display(root, generated_manifest) if generated_manifest else None
    findings: list[str] = []
    for path in sorted(unique):
        rel = display(root, path)
        if rel in {manifest_display, generated_manifest_display}:
            continue
        if token_cf in rel.casefold():
            key = (rel, "path", token_cf, rel)
            if key not in allowed:
                findings.append(f"{rel}: path contains forbidden identity")
        if path.is_symlink():
            target = os.readlink(path)
            if token_cf in target.casefold() and (rel, "symlink", token_cf, target) not in allowed:
                findings.append(f"{rel}: symlink target contains forbidden identity")
            continue
        text = read_text(path)
        if text is None:
            continue
        for line in text.splitlines():
            if (
                token_cf in line.casefold()
                and (rel, "content", token_cf, line) not in allowed
                and (rel, "content", token_cf) not in immutable
            ):
                findings.append(f"{rel}: content contains forbidden identity")
                break
    findings.extend(errors)
    if findings:
        for finding in sorted(set(findings)):
            print(finding, file=sys.stderr)
        return 1
    print(f"engine naming guard: OK ({len(unique)} files checked)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
