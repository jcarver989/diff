use clankerdiff_markdown::MarkdownDocument;
use clankerdiff_ratatui::{
    MarkdownRenderOptions, MarkdownRenderer, testing::MarkdownStreamFixture,
};
use clankerdiff_syntax::SyntaxHighlighter;
use clankerdiff_theme::{ReviewTheme, Rgba};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

type Operation = fn(&mut MarkdownStreamFixture, &str);

const OPERATIONS: [(&str, Operation); 4] = [
    ("append", |fixture, tail| {
        fixture.stream.push(tail);
    }),
    ("completion", |fixture, _| {
        fixture.stream.finish();
    }),
    ("resize", |fixture, _| fixture.options.width = 40),
    ("theme_change", |fixture, _| {
        fixture.theme.markdown.heading = Rgba::new(3, 5, 7, 255);
    }),
];

fn markdown(criterion: &mut Criterion) {
    let chat = "# Agent reply\n\n**Updated** the [`run`](https://example.com) command.\n\n```sh\necho \"$HOME\"\n```\n".to_owned();
    let long_fence = format!(
        "# Long context\n\n```rust\n/* open\n{}",
        "still comment\n".repeat(1_100)
    );
    for (name, source, tail) in [
        ("chat", chat, "\nDone."),
        ("long_fence", long_fence, "closed */\n```\n\nDone."),
    ] {
        let mut group = criterion.benchmark_group(format!("markdown/{name}"));
        group.bench_function("parse", |bencher| {
            bencher.iter(|| black_box(MarkdownDocument::parse(black_box(&source))));
        });
        let document = MarkdownDocument::parse(&source);
        group.bench_function("static_layout", |bencher| {
            let mut highlighter = SyntaxHighlighter::default();
            let theme = ReviewTheme::default();
            bencher.iter(|| {
                black_box(MarkdownRenderer::new().render_lines(
                    &document,
                    MarkdownRenderOptions::default(),
                    &theme,
                    &mut highlighter,
                ))
            });
        });
        let mut unchanged = warmed(&source);
        group.bench_function("unchanged", |bencher| {
            bencher.iter(|| black_box(unchanged.render()));
        });
        for (operation, apply) in OPERATIONS {
            group.bench_function(operation, |bencher| {
                bencher.iter_batched(
                    || warmed(&source),
                    |mut fixture| {
                        apply(&mut fixture, tail);
                        black_box(fixture.render())
                    },
                    BatchSize::SmallInput,
                );
            });
        }
        group.finish();
    }
}

fn warmed(source: &str) -> MarkdownStreamFixture {
    let mut fixture = MarkdownStreamFixture::default();
    fixture.stream.push(source);
    fixture.render();
    fixture
}

criterion_group!(benches, markdown);
criterion_main!(benches);
