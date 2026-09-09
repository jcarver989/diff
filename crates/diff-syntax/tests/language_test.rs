use clankerdiff_syntax::{
    Fingerprint, LanguageHint, SyntaxError, SyntaxHighlighter, SyntaxTheme, resolve_language,
};

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
        (".zshrc", "", Some("zsh")),
        ("go.mod", "", Some("go")),
        ("", "#!/usr/bin/env python3\n", Some("python")),
        ("", "#!/usr/bin/env node\n", Some("javascript")),
        ("unknown.bin", "bytes", None),
        ("markdown-inline", "", None),
    ] {
        assert_eq!(resolve_language(hint, source), expected, "{hint}");
    }
}

#[test]
fn bundled_languages_resolve_representative_families_and_exclude_nginx() -> Result<(), SyntaxError>
{
    let fixtures = [
        ("main.zig", "const x: u8 = 1;", "zig"),
        ("flake.nix", "{ pkgs, ... }: { }", "nix"),
        ("Main.hs", "main = putStrLn \"ok\"", "haskell"),
        ("app.exs", "IO.puts(\"ok\")", "elixir"),
        ("build.sbt", "val x = 1", "scala"),
        ("deps.edn", "{:deps {}}", "clojure"),
        ("main.ml", "let x = 1", "ocaml"),
        ("script.ps1", "Write-Host 'ok'", "powershell"),
        ("script.fish", "echo ok", "fish"),
        ("Makefile", "all:\n\techo ok", "make"),
        ("CMakeLists.txt", "project(foo)", "cmake"),
        ("build.ninja", "rule cc", "ninja"),
        ("main.tf", "resource \"x\" \"y\" {}", "hcl"),
        ("schema.graphql", "type Query { x: Int }", "graphql"),
        ("message.proto", "message X {}", "proto"),
        ("icon.svg", "<svg></svg>", "xml"),
        ("App.vue", "<template></template>", "vue"),
        ("App.svelte", "<script>let x = 1;</script>", "svelte"),
        ("style.scss", "$x: red;", "scss"),
        ("main.m", "@interface X @end", "objc"),
        ("lib.rs.patch", "+fn main() {}", "diff"),
    ];
    let theme = SyntaxTheme::default();
    let mut highlighter = SyntaxHighlighter::default();
    let mut styled_fixture_count = 0;
    for (path, source, expected) in fixtures {
        assert_eq!(
            resolve_language(LanguageHint::Path(path), source),
            Some(expected),
            "{path}"
        );
        let highlights = highlighter.with_theme(&theme).highlight_document(
            Fingerprint::of([source]),
            LanguageHint::Path(path),
            || source,
        )?;
        styled_fixture_count += usize::from((0..highlights.line_count()).any(|index| {
            highlights
                .line(index)
                .is_some_and(|spans| !spans.is_empty())
        }));
    }
    assert!(
        styled_fixture_count >= 15,
        "only {styled_fixture_count} fixtures highlighted"
    );
    assert_eq!(resolve_language("nginx", "server {}"), None);
    Ok(())
}
