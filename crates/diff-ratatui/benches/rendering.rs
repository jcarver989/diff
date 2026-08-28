#![allow(missing_docs)]

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use crossterm::event::KeyCode;
use diff_core::DiffDocument;
use diff_ratatui::DiffReviewState;
use std::{hint::black_box, sync::Arc};

#[path = "../tests/support/mod.rs"]
mod support;

use support::{ReviewHarness, key, large_document, many_file_document};

const SCROLL_STEPS: u64 = 100;
const PAGE_STEPS: u64 = 10;
const FILE_SWITCHES: u64 = 50;

fn presentation_creation(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("presentation_creation");
    group.sample_size(20);
    for rows in [1_000, 10_000, 100_000] {
        let document = large_document(rows);
        group.bench_with_input(BenchmarkId::from_parameter(rows), &rows, |bencher, _| {
            bencher.iter(|| DiffReviewState::new(black_box(document.clone())));
        });
    }
    group.finish();
}

fn rendering(criterion: &mut Criterion) {
    let document = large_document(10_000);
    let mut group = criterion.benchmark_group("visible_rendering_10k");
    group.sample_size(20);

    group.bench_function("cold_unified_80x24", |bencher| {
        bencher.iter_batched(
            || ReviewHarness::new(document.clone(), 80, 24),
            |mut harness| black_box(harness.draw()),
            BatchSize::SmallInput,
        );
    });

    let mut warm = ReviewHarness::new(document.clone(), 80, 24);
    warm.draw();
    group.bench_function("warm_unified_80x24", |bencher| {
        bencher.iter(|| black_box(warm.draw()));
    });

    let mut split = ReviewHarness::new(document.clone(), 140, 40);
    split.draw();
    group.bench_function("warm_split_140x40", |bencher| {
        bencher.iter(|| black_box(split.draw()));
    });

    let mut cached_navigation = ReviewHarness::new(document.clone(), 100, 24);
    cached_navigation.draw();
    cached_navigation.input(key(KeyCode::Enter));
    cached_navigation.draw();
    let mut down = true;
    group.bench_function("navigate_and_draw_100x24", |bencher| {
        bencher.iter(|| {
            let code = if down { KeyCode::Down } else { KeyCode::Up };
            down = !down;
            black_box(cached_navigation.input_and_draw(key(code)))
        });
    });
    group.finish();

    navigation(criterion, &document);
}

fn navigation(criterion: &mut Criterion, document: &Arc<DiffDocument>) {
    let mut group = criterion.benchmark_group("navigation_10k");
    group.sample_size(20);

    group.throughput(Throughput::Elements(SCROLL_STEPS));
    group.bench_function("scroll_100_new_rows_100x24", |bencher| {
        bencher.iter_batched(
            || patch_harness(document.clone(), 100, 24),
            |mut harness| {
                for _ in 0..SCROLL_STEPS {
                    black_box(harness.input_and_draw(key(KeyCode::Down)));
                }
            },
            BatchSize::SmallInput,
        );
    });

    group.throughput(Throughput::Elements(PAGE_STEPS));
    group.bench_function("page_down_10_new_viewports_100x24", |bencher| {
        bencher.iter_batched(
            || patch_harness(document.clone(), 100, 24),
            |mut harness| {
                for _ in 0..PAGE_STEPS {
                    black_box(harness.input_and_draw(key(KeyCode::PageDown)));
                }
            },
            BatchSize::SmallInput,
        );
    });

    let files = many_file_document(100, 100);
    group.throughput(Throughput::Elements(FILE_SWITCHES));
    group.bench_function("switch_50_files_100x24", |bencher| {
        bencher.iter_batched(
            || {
                let mut harness = ReviewHarness::new(files.clone(), 100, 24);
                harness.draw();
                harness
            },
            |mut harness| {
                for _ in 0..FILE_SWITCHES {
                    black_box(harness.input_and_draw(key(KeyCode::Down)));
                }
            },
            BatchSize::SmallInput,
        );
    });

    group.throughput(Throughput::Elements(2));
    group.bench_function("cycle_split_and_unified_140x40", |bencher| {
        bencher.iter_batched(
            || patch_harness(document.clone(), 140, 40),
            |mut harness| {
                black_box(harness.input_and_draw(key(KeyCode::Char('v'))));
                black_box(harness.input_and_draw(key(KeyCode::Char('v'))));
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn patch_harness(document: Arc<DiffDocument>, width: u16, height: u16) -> ReviewHarness {
    let mut harness = ReviewHarness::new(document, width, height);
    harness.draw();
    harness.input(key(KeyCode::Enter));
    harness.draw();
    harness
}

criterion_group!(benches, presentation_creation, rendering);
criterion_main!(benches);
