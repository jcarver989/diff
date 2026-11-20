#!/usr/bin/env python3
import argparse
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory


ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / "dist-workspace.toml"
WORKFLOW = ROOT / ".github/workflows/release.yml"
ALLOW_DIRTY = b'allow-dirty = ["ci"]\n'


def generate():
    original = CONFIG.read_bytes()
    if original.count(ALLOW_DIRTY) != 1:
        raise SystemExit("expected Framer's cargo-dist allow-dirty setting")
    previous = WORKFLOW.read_bytes() if WORKFLOW.exists() else None
    with TemporaryDirectory(prefix="diff-dist-", dir=ROOT.parent) as temp:
        workspace = Path(temp)
        (workspace / CONFIG.name).write_bytes(original.replace(ALLOW_DIRTY, b"", 1))
        for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml"):
            (workspace / name).symlink_to(ROOT / name)
        for manifest in ROOT.glob("crates/*/*/Cargo.toml"):
            crate = workspace / manifest.parent.relative_to(ROOT)
            crate.parent.mkdir(parents=True, exist_ok=True)
            crate.symlink_to(manifest.parent, target_is_directory=True)
        subprocess.run(["git", "init", "--quiet"], cwd=workspace, check=True)
        subprocess.run(["dist", "generate", "--mode", "ci"], cwd=workspace, check=True)
        workflow = (workspace / WORKFLOW.relative_to(ROOT)).read_bytes()
    return previous, workflow


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    previous, expected = generate()
    if args.check:
        if previous != expected:
            raise SystemExit("release workflow is out of date; run mise run dist-generate")
    else:
        WORKFLOW.write_bytes(expected)


if __name__ == "__main__":
    main()
