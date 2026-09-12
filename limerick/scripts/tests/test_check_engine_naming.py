#!/usr/bin/env python3
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).parents[1] / "check-engine-naming.py"
OLD = "pa" + "rish"


class NamingGuardTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / "limerick/scripts").mkdir(parents=True)
        self.manifest = self.root / "limerick/scripts/engine-naming-exceptions.json"
        self.write_manifest([])
        subprocess.run(["git", "init", "-q"], cwd=self.root, check=True)
        subprocess.run(
            ["git", "config", "user.email", "test@example.com"], cwd=self.root, check=True
        )
        subprocess.run(["git", "config", "user.name", "Test"], cwd=self.root, check=True)

    def tearDown(self):
        self.tmp.cleanup()

    def write_manifest(self, entries):
        self.manifest.parent.mkdir(parents=True, exist_ok=True)
        self.manifest.write_text(json.dumps({"exceptions": entries}), encoding="utf-8")

    def add(self, name, content=None, symlink=None):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        if symlink is not None:
            path.symlink_to(symlink)
        elif isinstance(content, bytes):
            path.write_bytes(content)
        else:
            path.write_text(content or "", encoding="utf-8")
        return path

    def invoke(self, *extra):
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--root", str(self.root), *extra],
            cwd=self.root,
            text=True,
            capture_output=True,
        )

    def test_hidden_source_path_and_symlink_are_found_including_untracked(self):
        self.add("hidden-" + OLD + ".toml", "secret = true")
        self.add("source.txt", "identity = '" + OLD + "'\n")
        self.add("link", symlink="target-" + OLD)
        result = self.invoke()
        self.assertEqual(result.returncode, 1)
        self.assertIn("hidden-" + OLD, result.stderr)
        self.assertIn("source.txt", result.stderr)
        self.assertIn("link", result.stderr)

    def test_exact_geographic_and_scanner_exceptions_allow_only_their_lines(self):
        self.add("world.json", '{"name":"' + OLD + '"}\n{"name":"' + OLD + '-new"}\n')
        first = '{"name":"' + OLD + '"}'
        self.write_manifest(
            [
                {
                    "path": "world.json",
                    "surface": "content",
                    "literal": OLD,
                    "context": first,
                    "category": "geographic-vocabulary",
                    "reason": "historical place name",
                }
            ]
        )
        result = self.invoke()
        self.assertEqual(result.returncode, 1)
        self.assertIn("world.json", result.stderr)

    def test_immutable_provenance_hash_allows_huge_line_without_context(self):
        path = self.add("capture.jsonl", '{"source":"' + OLD + '","payload":"' + OLD + '"}\n')
        self.write_manifest(
            [
                {
                    "path": "capture.jsonl",
                    "surface": "content",
                    "literal": OLD,
                    "category": "immutable-provenance",
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                    "reason": "archived capture",
                }
            ]
        )
        self.assertEqual(self.invoke().returncode, 0)

    def test_immutable_hash_mismatch_fails_closed(self):
        self.add("capture.jsonl", OLD + "\n")
        self.write_manifest(
            [
                {
                    "path": "capture.jsonl",
                    "surface": "content",
                    "literal": OLD,
                    "category": "immutable-provenance",
                    "sha256": "0" * 64,
                    "reason": "archived capture",
                }
            ]
        )
        result = self.invoke()
        self.assertEqual(result.returncode, 1)
        self.assertIn("hash mismatch", result.stderr)

    def test_stale_duplicate_and_traversal_entries_fail(self):
        entry = {
            "path": "missing.txt",
            "surface": "content",
            "literal": OLD,
            "context": OLD,
            "category": "scanner-test",
            "reason": "fixture",
        }
        self.write_manifest([entry, entry, {**entry, "path": "../outside"}])
        result = self.invoke()
        self.assertEqual(result.returncode, 1)
        self.assertIn("duplicate", result.stderr)
        self.assertIn("stale", result.stderr)
        self.assertIn("invalid path", result.stderr)

    def test_generated_root_scans_new_files_and_can_be_exactly_excepted(self):
        generated = self.root / "build/generated"
        generated.mkdir(parents=True)
        path = generated / "map.txt"
        path.write_text("historical " + OLD + "\n", encoding="utf-8")
        (generated / ("dir-" + OLD)).symlink_to(self.root / "elsewhere", target_is_directory=True)
        result = self.invoke("--generated-root", "build/generated")
        self.assertEqual(result.returncode, 1)
        self.assertIn("build/generated/map.txt", result.stderr)
        self.assertIn("build/generated/dir-" + OLD, result.stderr)
        (generated / ("dir-" + OLD)).unlink()
        line = "historical " + OLD
        self.write_manifest(
            [
                {
                    "path": "build/generated/map.txt",
                    "surface": "content",
                    "literal": OLD,
                    "context": line,
                    "category": "geographic-vocabulary",
                    "reason": "generated historical mapping",
                }
            ]
        )
        self.assertEqual(self.invoke("--generated-root", "build/generated").returncode, 0)

    def test_opaque_binary_is_documented_and_skipped(self):
        self.add("asset.bin", OLD.encode() + b"\0binary")
        result = self.invoke()
        self.assertEqual(result.returncode, 0)

    def test_manifest_schema_and_literal_are_strict(self):
        self.add("source.txt", OLD + "\n")
        self.write_manifest(
            [
                {
                    "path": "source.txt",
                    "surface": "content",
                    "literal": "old",
                    "context": OLD,
                    "category": "scanner-test",
                    "reason": "fixture",
                }
            ]
        )
        result = self.invoke()
        self.assertEqual(result.returncode, 1)
        self.assertIn("literal must be", result.stderr)
        self.write_manifest(
            [
                {
                    "path": "source.txt",
                    "surface": "content",
                    "literal": OLD,
                    "context": OLD,
                    "category": "scanner-test",
                    "reason": "fixture",
                    "wildcard": "*",
                }
            ]
        )
        self.assertIn("unknown fields", self.invoke().stderr)

    def test_exception_without_current_candidate_is_stale(self):
        self.add("source.txt", "clean\n")
        self.write_manifest(
            [
                {
                    "path": "source.txt",
                    "surface": "content",
                    "literal": OLD,
                    "context": OLD,
                    "category": "scanner-test",
                    "reason": "fixture",
                }
            ]
        )
        result = self.invoke()
        self.assertEqual(result.returncode, 1)
        self.assertIn("no candidate", result.stderr)

    def test_external_symlink_is_scanned_as_link_but_target_is_never_read(self):
        outside = Path(self.tmp.name).parent / (self.root.name + "-outside")
        outside.write_text("secret " + OLD, encoding="utf-8")
        try:
            self.add("external-link", symlink=str(outside))
            result = self.invoke()
            self.assertEqual(result.returncode, 0)
            self.assertNotIn("content contains", result.stderr)
        finally:
            outside.unlink()

    def test_generated_inventory_requires_root_membership_and_hash(self):
        generated = self.root / "build/dist"
        generated.mkdir(parents=True)
        path = generated / "bundle.js"
        path.write_text("const place='" + OLD + "';const other='" + OLD + "';", encoding="utf-8")
        inventory = self.root / "generated-exceptions.json"
        inventory.write_text(
            json.dumps(
                {
                    "exceptions": [
                        {
                            "path": "build/dist/bundle.js",
                            "surface": "content",
                            "literal": OLD,
                            "category": "geographic-vocabulary",
                            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                            "reason": "reviewed generated geographic payload",
                        }
                    ]
                }
            ),
            encoding="utf-8",
        )
        result = self.invoke(
            "--generated-root", "build/dist", "--generated-exceptions", str(inventory)
        )
        self.assertEqual(result.returncode, 0)
        path.write_text(path.read_text(encoding="utf-8") + "!", encoding="utf-8")
        result = self.invoke(
            "--generated-root", "build/dist", "--generated-exceptions", str(inventory)
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("generated content hash mismatch", result.stderr)

    def test_generated_inventory_cannot_be_used_without_explicit_root(self):
        inventory = self.root / "generated-exceptions.json"
        inventory.write_text(json.dumps({"exceptions": []}), encoding="utf-8")
        result = self.invoke("--generated-exceptions", str(inventory))
        self.assertEqual(result.returncode, 2)
        self.assertIn("requires", result.stderr)

    def test_generated_inventory_rejects_wrong_root_and_category(self):
        generated = self.root / "build/dist"
        generated.mkdir(parents=True)
        path = generated / "bundle.js"
        path.write_text("const place='" + OLD + "';", encoding="utf-8")
        inventory = self.root / "generated-exceptions.json"
        inventory.write_text(
            json.dumps(
                {
                    "exceptions": [
                        {
                            "path": "source.txt",
                            "surface": "content",
                            "literal": OLD,
                            "category": "scanner-test",
                            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                            "reason": "bad generated record",
                        }
                    ]
                }
            ),
            encoding="utf-8",
        )
        result = self.invoke(
            "--generated-root", "build/dist", "--generated-exceptions", str(inventory)
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("outside enumerated generated files", result.stderr)
        self.assertIn("unsupported generated category", result.stderr)

    def test_generated_binary_path_exception_is_hash_bound(self):
        generated = self.root / "build/ui"
        generated.mkdir(parents=True)
        path = generated / ("assets-" + OLD + "-crossroads-watercolor.png")
        path.write_bytes(b"\x89PNG\r\n" + OLD.encode() + b"\0opaque")
        inventory = self.root / "generated-exceptions.json"
        rel = "build/ui/assets-" + OLD + "-crossroads-watercolor.png"
        inventory.write_text(
            json.dumps(
                {
                    "exceptions": [
                        {
                            "path": rel,
                            "surface": "path",
                            "literal": OLD,
                            "context": rel,
                            "category": "geographic-vocabulary",
                            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                            "reason": "reviewed geographic asset filename",
                        }
                    ]
                }
            ),
            encoding="utf-8",
        )
        result = self.invoke(
            "--generated-root", "build/ui", "--generated-exceptions", str(inventory)
        )
        self.assertEqual(result.returncode, 0)
        path.write_bytes(path.read_bytes() + b"tampered")
        result = self.invoke(
            "--generated-root", "build/ui", "--generated-exceptions", str(inventory)
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("generated content hash mismatch", result.stderr)


if __name__ == "__main__":
    unittest.main()
