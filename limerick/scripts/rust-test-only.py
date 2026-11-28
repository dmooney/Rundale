#!/usr/bin/env python3
"""Decide whether a change to one Rust file is confined to test-only code.

Usage: rust-test-only.py BASE PATH   (run from the repository root)

Exit 0 when every changed line of PATH, between BASE and the working tree, is
provably test-only; exit 1 otherwise. Used by agent-check.sh so that editing
only the `#[cfg(test)]` module of a runtime-path file needs no evidence page.

Conservative by design: anything that cannot be proven test-only (a parse
oddity, a missing file, an empty diff) exits 1, so the file stays runtime.

Test-only means one of:
  - the whole file is test-gated: it starts with `#![cfg(test)]`, or its
    declaring `mod` item (in lib.rs, main.rs, mod.rs, or `<dir>.rs`, searched
    transitively upward) carries `#[cfg(test)]`, on every side that has the
    file;
  - each changed line, on the old side and the new side, lies inside a
    `#[cfg(test)] mod name { ... }` block (or a `#[cfg(test)] mod name;`
    declaration) of that side's content.
Adding or removing the gate itself on code that stays is a runtime change.
Only the exact attribute `#[cfg(test)]` counts; `cfg(any(test, ..))` and
`cfg(not(test))` do not.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from collections.abc import Callable

CFG_TEST = r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]"
ATTRS = r"(?:\s*#\s*\[[^\]]*\])*"
VIS = r"(?:pub(?:\s*\([^)]*\))?\s+)?"
TEST_MOD = re.compile(CFG_TEST + ATTRS + r"\s*" + VIS + r"mod\s+(\w+)\s*([{;])")
INNER_CFG_TEST = re.compile(r"\A\s*#\s*!\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]")
CHAR_LIT = re.compile(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}|.)|[^\\'\n])'", re.S)
RAW_START = re.compile(r"(?:b?r)(#*)\"")
HUNK = re.compile(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", re.M)


class Unparseable(Exception):
    pass


def mask(src: str) -> str:
    """Blank comments and string/char literal bodies, keeping length and newlines."""
    out = list(src)
    n = len(src)
    i = 0

    def blank(a: int, b: int) -> None:
        for k in range(a, b):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        c = src[i]
        if src.startswith("//", i):
            j = src.find("\n", i)
            j = n if j < 0 else j
            blank(i, j)
            i = j
        elif src.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif src.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            if depth:
                raise Unparseable("unterminated block comment")
            blank(i, j)
            i = j
        elif c in "rb" and (i == 0 or not (src[i - 1].isalnum() or src[i - 1] == "_")):
            m = RAW_START.match(src, i)
            if m:
                end = src.find('"' + m.group(1), m.end())
                if end < 0:
                    raise Unparseable("unterminated raw string")
                blank(m.end(), end)
                i = end + 1 + len(m.group(1))
            elif src.startswith('b"', i):
                i += 1  # fall through to the string branch next loop
            else:
                i += 1
        elif c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            if j >= n:
                raise Unparseable("unterminated string")
            blank(i + 1, j)
            i = j + 1
        elif c == "'":
            m = CHAR_LIT.match(src, i)
            if m:
                blank(i + 1, m.end() - 1)
                i = m.end()
            else:
                i += 1  # a lifetime
        else:
            i += 1
    return "".join(out)


def test_spans(src: str) -> list[tuple[int, int, int]]:
    """(first, last, header_last) 1-based lines of each `#[cfg(test)] mod` item.

    The header runs from the attribute through the `{` or `;` that ends the
    `mod` line; the span runs on to the closing brace.
    """
    masked = mask(src)
    spans: list[tuple[int, int, int]] = []
    for m in TEST_MOD.finditer(masked):
        start = m.start()
        if m.group(2) == ";":
            end = m.end() - 1
        else:
            depth, j = 1, m.end()
            while j < len(masked) and depth:
                depth += (masked[j] == "{") - (masked[j] == "}")
                j += 1
            if depth:
                raise Unparseable("unbalanced braces in test module")
            end = j - 1
        first = masked.count("\n", 0, start) + 1
        last = masked.count("\n", 0, end) + 1
        header_last = masked.count("\n", 0, m.end() - 1) + 1
        spans.append((first, last, header_last))
    return spans


def side_ok(src: str | None, changed: set[int]) -> bool:
    """Every changed line of one side is a blank or inside a test-module span.

    Touching a span's header (the `#[cfg(test)]` attribute or `mod` line) or
    its closing line (moving the brace changes what the gate covers)
    counts only when the whole span is changed with it: adding or removing the
    gate on code that stays turns runtime code into test code or back.
    """
    if not changed:
        return True
    if src is None:
        return False
    spans = test_spans(src)
    covered = {i + 1 for i, line in enumerate(src.split("\n")) if not line.strip()}
    for first, last, header_last in spans:
        covered.update(range(first, last + 1))
        if (
            changed & (set(range(first, header_last + 1)) | {last})
            and not set(range(first, last + 1)) <= changed
        ):
            return False
    return changed <= covered


def git_show(base: str, path: str) -> str | None:
    r = subprocess.run(["git", "show", f"{base}:{path}"], capture_output=True)
    return r.stdout.decode("utf-8", "replace") if r.returncode == 0 else None


def read_work(path: str) -> str | None:
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            return f.read()
    except OSError:
        return None


def declaring_files(path: str) -> tuple[str, list[str]]:
    """(module name, candidate files that could declare it) for a Rust file."""
    d, fname = os.path.split(path)
    stem = fname[:-3]
    if stem == "mod":
        name = os.path.basename(d)
        d = os.path.dirname(d)
    else:
        name = stem
    cands = [os.path.join(d, x) for x in ("mod.rs", "lib.rs", "main.rs")]
    parent, base = os.path.split(d)
    if base:
        cands.append(os.path.join(parent, base + ".rs"))
    return name, [c for c in cands if c != path]


def whole_file_gated(path: str, reader, depth: int = 0) -> bool:
    """True if `path` is compiled only under cfg(test), per `reader`'s tree."""
    if depth > 6 or not path.endswith(".rs"):
        return False
    text = reader(path)
    if text is None:
        return False
    try:
        if INNER_CFG_TEST.match(mask(text)):
            return True
    except Unparseable:
        return False
    if os.path.basename(path) in ("lib.rs", "main.rs"):
        return False
    name, cands = declaring_files(path)
    decl = re.compile(CFG_TEST + ATTRS + r"\s*" + VIS + r"mod\s+" + re.escape(name) + r"\s*;")
    for cand in cands:
        ctext = reader(cand)
        if ctext is None:
            continue
        try:
            if decl.search(mask(ctext)):
                return True
            if re.search(r"\bmod\s+" + re.escape(name) + r"\s*;", mask(ctext)):
                # Declared here without the gate; a gated parent may still cover it.
                return whole_file_gated(cand, reader, depth + 1)
        except Unparseable:
            return False
    return False


def hunks(base: str, path: str, old: str | None, new: str | None):
    r = subprocess.run(
        ["git", "diff", "-U0", "--no-color", "--no-ext-diff", "--no-renames", base, "--", path],
        capture_output=True,
    )
    diff = r.stdout.decode("utf-8", "replace")
    found = HUNK.findall(diff)
    if not found and old is None and new is not None:
        # Untracked new file: every line is added.
        return [(0, 0, 1, new.count("\n") + (0 if new.endswith("\n") else 1))]
    out = []
    for a, b, c, d in found:
        out.append((int(a), int(b) if b != "" else 1, int(c), int(d) if d != "" else 1))
    return out


def test_only(base: str, path: str) -> bool:
    if not path.endswith(".rs"):
        return False
    old = git_show(base, path)
    new = read_work(path)
    if old is None and new is None:
        return False

    def old_reader(p: str) -> str | None:
        return git_show(base, p)

    sides: list[Callable[[str], str | None]] = []
    if old is not None:
        sides.append(old_reader)
    if new is not None:
        sides.append(read_work)
    if all(whole_file_gated(path, rd) for rd in sides):
        return True

    hs = hunks(base, path, old, new)
    if not hs:
        return False
    old_changed: set[int] = set()
    new_changed: set[int] = set()
    for a, b, c, d in hs:
        old_changed.update(range(a, a + b) if b else ())
        new_changed.update(range(c, c + d) if d else ())
    return side_ok(old, old_changed) and side_ok(new, new_changed)


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: rust-test-only.py BASE PATH", file=sys.stderr)
        return 2
    try:
        return 0 if test_only(sys.argv[1], sys.argv[2]) else 1
    except Unparseable:
        return 1


if __name__ == "__main__":
    sys.exit(main())
