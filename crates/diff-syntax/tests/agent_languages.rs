#![cfg(feature = "agent-languages")]

use diff_syntax::{LanguageHint, SyntaxHighlighter, resolve_language};
use diff_theme::SyntaxTheme;

#[test]
fn agent_bundle_resolves_representative_families_and_excludes_nginx() {
    let fixtures = [
        ("main.zig", "const x: u8 = 1;", "zig"),
        ("flake.nix", "{ pkgs, ... }: { }", "nix"),
        ("Main.hs", "main = putStrLn \"ok\"", "haskell"),
        ("app.exs", "IO.puts(\"ok\")", "elixir"),
        ("build.sbt", "val x = 1", "scala"),
        ("deps.edn", "{:deps {}}", "clojure"),
        ("main.ml", "let x = 1", "ocaml"),
        ("Program.fs", "let x = 1", "fsharp"),
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
        let spans = highlighter
            .with_theme(&theme)
            .highlight_source(LanguageHint::Path(path), source);
        styled_fixture_count += usize::from(!spans.is_empty());
    }
    assert!(
        styled_fixture_count >= 15,
        "only {styled_fixture_count} fixtures highlighted"
    );
    assert_eq!(resolve_language("nginx", "server {}"), None);
}
