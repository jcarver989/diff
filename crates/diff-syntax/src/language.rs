//! Deterministic resolution of language hints to Arborium grammar IDs.

/// A structured language hint accepted by the highlighter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LanguageHint<'a> {
    Id(&'a str),
    InfoString(&'a str),
    Path(&'a str),
    Auto,
}

impl<'a> From<&'a str> for LanguageHint<'a> {
    fn from(value: &'a str) -> Self {
        Self::Id(value)
    }
}

impl<'a> LanguageHint<'a> {
    /// Flattens the hint to the token [`resolve_language`] matches against.
    #[must_use]
    pub fn as_str(&self) -> &'a str {
        match self {
            Self::Id(value) | Self::Path(value) => value,
            Self::InfoString(value) => value.split_ascii_whitespace().next().unwrap_or_default(),
            Self::Auto => "",
        }
    }
}

/// Resolves a language ID, alias, or repository path to a bundled Arborium grammar.
pub fn resolve_language<'a>(
    hint: impl Into<LanguageHint<'a>>,
    source: &str,
) -> Option<&'static str> {
    let hint = hint.into().as_str();
    let normalized = hint.trim().replace('\\', "/").to_ascii_lowercase();
    if normalized.is_empty() {
        return shebang_id(source);
    }

    let simple = normalized.rsplit('/').next().unwrap_or(&normalized);
    canonical_id(&normalized)
        .or_else(|| canonical_id(simple))
        .or_else(|| extension_id(simple))
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
        "csharp" | "c-sharp" | "c#" | "cs" => "c-sharp",
        "cpp" | "c++" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
        "go" | "golang" => "go",
        "java" => "java",
        "kotlin" | "kt" | "kts" => "kotlin",
        "ruby" | "rb" => "ruby",
        "swift" => "swift",
        "php" => "php",
        "sql" => "sql",
        "lua" => "lua",
        "dockerfile" => "dockerfile",
        "json" | "jsonc" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "html" | "htm" => "html",
        "css" => "css",
        "markdown" | "md" => "markdown",
        "zig" => "zig",
        "nix" => "nix",
        "haskell" | "hs" => "haskell",
        "elixir" | "ex" | "exs" => "elixir",
        "erlang" | "erl" | "hrl" => "erlang",
        "scala" | "sc" => "scala",
        "clojure" | "clj" | "cljs" | "cljc" | "edn" => "clojure",
        "commonlisp" | "common-lisp" | "lisp" | "cl" => "commonlisp",
        "scheme" | "scm" | "ss" => "scheme",
        "ocaml" | "ml" | "mli" => "ocaml",
        "fsharp" | "f#" | "fs" | "fsi" | "fsx" => "fsharp",
        "dart" => "dart",
        "powershell" | "pwsh" | "ps1" | "psm1" => "powershell",
        "fish" => "fish",
        "make" | "makefile" => "make",
        "cmake" => "cmake",
        "ninja" => "ninja",
        "meson" => "meson",
        "just" | "justfile" => "just",
        "hcl" | "terraform" | "tf" | "tfvars" => "hcl",
        "graphql" | "gql" => "graphql",
        "protobuf" | "proto" => "proto",
        "xml" | "xhtml" | "svg" => "xml",
        "vue" => "vue",
        "svelte" => "svelte",
        "scss" => "scss",
        "asm" | "assembly" => "asm",
        "x86asm" | "x86-asm" | "nasm" => "x86asm",
        "objective-c" | "objectivec" | "objc" => "objc",
        "perl" | "pl" | "pm" => "perl",
        "r" => "r",
        "solidity" | "sol" => "solidity",
        "starlark" | "bzl" | "bazel" => "starlark",
        "rego" => "rego",
        "ini" | "cfg" => "ini",
        "diff" | "patch" => "diff",
        _ => return None,
    })
}

fn extension_id(file: &str) -> Option<&'static str> {
    let extension = file.rsplit_once('.')?.1;
    canonical_id(extension).or(match extension {
        // Meaningful as file extensions but too ambiguous to honor as bare
        // language IDs or fence info strings.
        "s" => Some("asm"),
        "m" | "mm" => Some("objc"),
        _ => None,
    })
}

fn special_file(file: &str) -> Option<&'static str> {
    match file {
        "dockerfile" | "containerfile" => Some("dockerfile"),
        "go.mod" | "go.sum" => Some("go"),
        "makefile" | "gnumakefile" => Some("make"),
        "cmakelists.txt" => Some("cmake"),
        "build.ninja" => Some("ninja"),
        "meson.build" | "meson_options.txt" => Some("meson"),
        "justfile" => Some("just"),
        "flake.nix" => Some("nix"),
        ".terraformrc" => Some("hcl"),
        "workspace" => Some("starlark"),
        "build.sbt" => Some("scala"),
        "deps.edn" => Some("clojure"),
        ".zshrc" if cfg!(feature = "agent-languages") => Some("zsh"),
        ".bashrc" | ".zshrc" => Some("bash"),
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
    } else if lower.contains("fish") {
        Some("fish")
    } else if lower.contains("pwsh") || lower.contains("powershell") {
        Some("powershell")
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
            ("Dockerfile", "", Some("dockerfile")),
            ("Containerfile", "", Some("dockerfile")),
            ("Program.cs", "", Some("c-sharp")),
            ("Main.java", "", Some("java")),
            ("Main.kt", "", Some("kotlin")),
            ("script.rb", "", Some("ruby")),
            ("query.sql", "", Some("sql")),
            (".bashrc", "", Some("bash")),
            (
                ".zshrc",
                "",
                Some(if cfg!(feature = "agent-languages") {
                    "zsh"
                } else {
                    "bash"
                }),
            ),
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
