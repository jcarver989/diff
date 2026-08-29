//! Deterministic resolution of language hints to Arborium grammar IDs.

/// Resolves a language ID, alias, or repository path to a bundled Arborium grammar.
pub(crate) fn resolve_language(hint: &str, source: &str) -> Option<&'static str> {
    let normalized = hint.trim().replace('\\', "/").to_ascii_lowercase();
    if normalized.is_empty() {
        return shebang_id(source);
    }

    let simple = normalized.rsplit('/').next().unwrap_or(&normalized);
    canonical_id(&normalized)
        .or_else(|| canonical_id(simple))
        .or_else(|| special_file(simple))
        .or_else(|| arborium::detect_language(&normalized).and_then(canonical_id))
        .or_else(|| shebang_id(source))
}

fn canonical_id(hint: &str) -> Option<&'static str> {
    Some(match hint {
        "rust" | "rs" => "rust",
        "javascript" | "js" | "mjs" | "cjs" | "node" | "jsx" => "javascript",
        "typescript" | "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "python" | "py" | "python3" => "python",
        "bash" | "sh" | "shell" | "zsh" => "bash",
        "c" | "h" => "c",
        "cpp" | "c++" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
        "go" | "golang" => "go",
        "json" | "jsonc" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "html" | "htm" => "html",
        "css" => "css",
        "markdown" | "md" => "markdown",
        _ => return None,
    })
}

fn special_file(file: &str) -> Option<&'static str> {
    match file {
        "dockerfile" | "containerfile" | ".bashrc" | ".zshrc" => Some("bash"),
        "go.mod" | "go.sum" => Some("go"),
        _ => None,
    }
}

fn shebang_id(source: &str) -> Option<&'static str> {
    let line = source.lines().next()?;
    if !line.starts_with("#!") {
        return None;
    }
    let lower = line.to_ascii_lowercase();
    if lower.contains("python") {
        Some("python")
    } else if lower.contains("node") {
        Some("javascript")
    } else if lower.contains("bash") || lower.contains("/sh") || lower.contains("zsh") {
        Some("bash")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_aliases_paths_special_files_and_shebangs() {
        for (hint, source, expected) in [
            ("RUST", "", Some("rust")),
            (".rs", "", Some("rust")),
            ("src/lib.rs", "", Some("rust")),
            ("src\\lib.rs", "", Some("rust")),
            ("view.tsx", "", Some("tsx")),
            ("x.d.ts", "", Some("typescript")),
            ("x.d.mts", "", Some("typescript")),
            ("x.d.cts", "", Some("typescript")),
            ("foo.jsx", "", Some("javascript")),
            ("foo.jsonc", "", Some("json")),
            ("foo.yml", "", Some("yaml")),
            ("Dockerfile", "", Some("bash")),
            ("Containerfile", "", Some("bash")),
            (".bashrc", "", Some("bash")),
            (".zshrc", "", Some("bash")),
            ("go.mod", "", Some("go")),
            ("", "#!/usr/bin/env python3\n", Some("python")),
            ("", "#!/usr/bin/env node\n", Some("javascript")),
            ("unknown.bin", "bytes", None),
            ("markdown-inline", "", None),
        ] {
            assert_eq!(resolve_language(hint, source), expected, "{hint}");
        }
    }
}
