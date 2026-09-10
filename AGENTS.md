# Diff

This is a repository for Diff, a performant diff tool with comment support. The CLI runs in a TUI (ratatui) or Desktop (gpui), while the standalone Web frontend (gpui) supports hosted use cases.

## Coding conventions

### General

1. Prefer using `Foo` over `std::biz::baz::boo::Foo` in code by importing types at the top of the file, e.g. `use std::biz::baz::boo::Foo`.
2. Prefer using `T`, `U`, `V` etc for generic type param names, always start with `T`.
3. Use `thiserror` crate for errors.
4. Avoid `Mutex`, `Arc<Mutex>`, `Semaphore` and other forms of locking where possible. Instead give each task (or thread) ownership of its own resources, or if you must share resources use either structured concurrency or tokio channels via an actor pattern.
5. Crates in this workspace should have "one way to do something" and export a cohesive api -- e.g. if we need to support incremental syntax highlighting _and_ full file syntax highlighting, the incremental API should be designed to cover both use-cases and we should not create parallel implementations.

### Rust docs

1. Never add comments unless explicitly instructed to. 

### Testing

1. Use real objects where possible. When it's not possible, prefer crate-provided test utilities and/or in-memory fakes over mocks.
2. Use the test builder pattern described in this [post](https://jmmv.dev/2020/12/builder-pattern-for-tests.html).
3. Prefer extending an existing fake or test builder to support a use case over creating a new bespoke builder/fake. Good builders and fakes are general purpose, reusable across test suites, and mimic the behavior and APIs of the "real thing".
4. Tests may only test _public_ APIs; never private. Thus, prefer integration tests in `tests/` over unit tests.
5. Put non-shared test helpers at the _bottom_ of files, below the tests.
6. Prefer using `?` in tests by making the test return a `Result` vs using `unwrap`, which is an anti-pattern. 
7. Prefer integration tests in the crate's `tests/`dir over unit tests. Integration test files should match the name and directory structure of the file they test, e.g `src/foo/boo.rs` and `test/foo/boo_test.rs`.
