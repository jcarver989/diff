import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SURFACES = (
    "crates/ui/gpui",
    "crates/ui/gpui-shell",
    "crates/diff/gpui",
    "crates/diff/desktop",
    "crates/diff/web",
)


def git(*args, cwd=ROOT):
    return subprocess.check_output(["git", *args], cwd=cwd, text=True)


def digest(text):
    return hashlib.sha256(text.encode()).hexdigest()


def code_only(text, keep_strings=False):
    token = re.compile(r"""\br(?P<hashes>\#*)"[\s\S]*?"(?P=hashes)|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\\n])'|`(?:\\.|[^`\\])*`|//[^\n]*|/\*""")
    pieces = list(text)
    position = 0
    while match := token.search(text, position):
        start, end = match.span()
        comment = match[0].startswith(("//", "/*"))
        if match[0] == "/*":
            depth = 1
            while depth and end < len(text):
                boundary = re.search(r"/\*|\*/", text[end:])
                if boundary is None:
                    end = len(text)
                    break
                depth += 1 if boundary[0] == "/*" else -1
                end += boundary.end()
        if comment or not keep_strings:
            pieces[start:end] = ["\n" if char == "\n" else " " for char in text[start:end]]
        position = end
    return "".join(pieces)


def declarations(text):
    code = code_only(text)
    pattern = r"\bpub\s+(?:(?:async|const|unsafe)\s+)*(mod|use|struct|enum|trait|type|fn|const|static)\s+([A-Za-z_]\w*)"
    for match in re.finditer(pattern, code):
        kind, name = match.groups()
        if kind == "use":
            end = code.index(";", match.start()) + 1
        else:
            ends = [pos for token in (";", "{") if (pos := code.find(token, match.end())) >= 0]
            end = min(ends) if ends else match.end()
        declaration = " ".join(text[match.start():end].split())
        yield {"kind": kind, "name": name, "line": text.count("\n", 0, match.start()) + 1, "declaration": declaration}


def exported_names(declaration):
    declaration = re.sub(r"^pub\s+use\s+|;$", "", declaration)
    names = []
    for part in re.split(r"[,{}]", declaration):
        part = part.strip()
        if not part or part.endswith("::"):
            continue
        names.append(part.rsplit(" as ", 1)[-1].rsplit("::", 1)[-1])
    return names


def target_path(path):
    return path.replace("/gpui-shell/", "/rouge-shell/").replace("/gpui/", "/rouge/")


def inventory(revision):
    files = git("ls-tree", "-r", "--name-only", revision).splitlines()
    sources = {path: git("show", f"{revision}:{path}") for path in files if path.endswith((".rs", ".ts")) and any(path.startswith(f"{surface}/src/") or path.startswith(f"{surface}/tests/") for surface in SURFACES)}
    api = []
    tests = []
    for path, text in sorted(sources.items()):
        if path.endswith(".rs") and "/src/" in path:
            entries = list(declarations(text))
            for entry in entries:
                if entry["kind"] == "use":
                    entry["exported_names"] = exported_names(entry["declaration"])
            api.append({"file": path, "sha256": digest(text), "declarations": entries})
        rust = r"#\[(?:(?:gpui|tokio)::)?test(?:\([^\]]*\))?\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)"
        pattern = rust if path.endswith(".rs") else r'\btest\(\s*["\']([^"\']+)["\']'
        for match in re.finditer(pattern, code_only(text, keep_strings=path.endswith(".ts"))):
            name = match[1]
            destination = target_path(path)
            if "/src/" in destination:
                prefix, module = destination.split("/src/", 1)
                module = module.removesuffix(".rs").replace("/", "_")
                destination = f"{prefix}/tests/{'host' if module == 'lib' else module}_contract_test.rs"
            tests.append({
                "id": f"{path}::{name}", "source_file": path,
                "source_line": text.count("\n", 0, match.start()) + 1,
                "source_sha256": digest(text), "name": name,
                "behavior": name.replace("_", " "), "replacement_file": destination,
                "requirement": "preserve public behavior; port test or document an intentional removal",
                "port_status": "required_not_yet_ported",
            })
    return {
        "api.json": {"schema_version": 1, "revision": revision, "scope": "Public declarations in the five renderer source trees, excluding comments and string contents, including declarations in private implementation modules; lib.rs declarations define the root exports. Names from actions! macros are captured through their root reexports, not macro expansion. This is a source inventory, not a semver-compatible API promise.", "files": api},
        "test-map.json": {"schema_version": 1, "revision": revision, "test_count": len(tests), "scope": "Lexical inventory excluding comments. All explicit #[test], #[gpui::test], #[tokio::test], and test(...) declarations in the five renderer source/test trees. Parameterized cases remain under their source test. Replacement paths are planned public integration targets, not claims of completed ports.", "tests": tests},
    }


def consumers():
    result = []
    for name in ("j-terminal", "aether"):
        root = ROOT.parent / name
        if not (root / ".git").exists():
            result.append({"repository": name, "available": False})
            continue
        references = []
        paths = git("ls-files", "--", "*.rs", "*Cargo.toml", cwd=root).splitlines()
        for path in paths:
            file = root / path
            if not file.is_file():
                continue
            text = file.read_text()
            for match in re.finditer(r"(?:pub\s+)?use\s+clankerdiff_gpui(?:_ui|_shell)?\b[^;]*;|^.*(?:clankerdiff-gpui|GpuiApp|diff_framer).*$", text, re.M):
                references.append({"file": path, "line": text.count("\n", 0, match.start()) + 1, "source": match[0].strip(), "file_sha256": digest(text)})
        result.append({"repository": name, "available": True, "revision": git("rev-parse", "HEAD", cwd=root).strip(), "tracked_worktree_dirty": bool(git("status", "--porcelain", "--untracked-files=no", cwd=root)), "references": references})
    return {"schema_version": 1, "scope": "Actual current sibling worktrees: package declarations and explicit imports. Glob imports require the accompanying consumer checklist; lexical import evidence is not a resolved-symbol usage graph. Consumers are read only and are not migrated by baseline capture.", "consumers": result}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--revision", default="09a11f7e7fbaa354a2df4743b4ee18ede000a30c")
    parser.add_argument("--output", type=Path, default=ROOT / "docs/migration-baseline")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    revision = git("rev-parse", args.revision).strip()
    documents = inventory(revision)
    if not args.check:
        documents["consumers.json"] = consumers()
    args.output.mkdir(parents=True, exist_ok=True)
    for name, document in documents.items():
        rendered = json.dumps(document, ensure_ascii=False, indent=2) + "\n"
        path = args.output / name
        if args.check:
            if path.read_text() != rendered:
                raise SystemExit(f"baseline inventory differs: {path}")
        else:
            path.write_text(rendered)
    print(f"{'Verified' if args.check else 'Recorded'} {len(documents['api.json']['files'])} source files and {documents['test-map.json']['test_count']} tests at {revision}")


if __name__ == "__main__":
    main()
