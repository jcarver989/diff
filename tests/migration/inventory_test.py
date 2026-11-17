import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/migration/inventory.py"
REVISION = "09a11f7e7fbaa354a2df4743b4ee18ede000a30c"


class InventoryTest(unittest.TestCase):
    def test_curated_browser_artifacts_match_recorded_hashes(self):
        baseline = ROOT / "docs/migration-baseline/browser"
        results = json.loads((baseline / "results.json").read_text())
        self.assertEqual(results["source_revision"], REVISION)
        self.assertEqual(results["capture_script_sha256"], hashlib.sha256((ROOT / "scripts/migration/capture-browser.mjs").read_bytes()).hexdigest())
        self.assertEqual(len(results["screenshots"]), 8)
        for image in results["screenshots"]:
            data = (baseline / image["file"]).read_bytes()
            self.assertEqual(image["sha256"], hashlib.sha256(data).hexdigest())
            self.assertEqual(image["bytes"], len(data))
            self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
        for name, digest in results["fixture_sha256"].items():
            fixture = ROOT / "tests/fixtures/migration" / name
            if fixture.exists():
                self.assertEqual(digest, hashlib.sha256(fixture.read_bytes()).hexdigest())
        self.assertFalse(results["errors"])

    def test_cli_records_exact_source_hashes_and_detects_changed_inventory(self):
        with tempfile.TemporaryDirectory() as directory:
            result = run_inventory(directory)
            self.assertEqual(result.returncode, 0, result.stderr)
            api_path = Path(directory) / "api.json"
            api = json.loads(api_path.read_text())
            self.assertEqual(api["revision"], REVISION)
            for source in api["files"]:
                original = subprocess.check_output(["git", "show", f"{REVISION}:{source['file']}"], cwd=ROOT)
                self.assertEqual(source["sha256"], hashlib.sha256(original).hexdigest())
            exports = next(source for source in api["files"] if source["file"] == "crates/diff/gpui/src/lib.rs")
            names = {name for declaration in exports["declarations"] for name in declaration.get("exported_names", [])}
            self.assertTrue({"DiffViewer", "DiffViewerEvent", "MarkdownReviewer", "ui"} <= names)
            tests = json.loads((Path(directory) / "test-map.json").read_text())
            self.assertEqual(tests["test_count"], len(tests["tests"]))
            self.assertEqual(len({test["id"] for test in tests["tests"]}), tests["test_count"])
            self.assertTrue(all(test["replacement_file"] and test["port_status"] == "required_not_yet_ported" for test in tests["tests"]))
            self.assertTrue(any(test["source_file"].endswith("web.spec.ts") for test in tests["tests"]))
            self.assertTrue(any(test["source_file"].endswith("src/viewer.rs") for test in tests["tests"]))
            self.assertEqual(run_inventory(directory, "--check").returncode, 0)
            api_path.write_text("{}\n")
            mismatch = run_inventory(directory, "--check")
            self.assertNotEqual(mismatch.returncode, 0)
            self.assertIn("baseline inventory differs", mismatch.stderr)


def run_inventory(directory, *arguments):
    return subprocess.run([sys.executable, str(SCRIPT), "--output", directory, *arguments], cwd=ROOT, text=True, capture_output=True)


if __name__ == "__main__":
    unittest.main()
