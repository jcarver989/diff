# Diff

This is a repository for Diff, a performant diff tool with comment support. The CLI runs in a TUI (ratatui) or Desktop (gpui), while the standalone Web frontend (gpui) supports hosted use cases.

## Coding conventions

### General

1. Prefer using `Foo` over `std::biz::baz::boo::Foo` in code by importing types at the top of the file, e.g. `use std::biz::baz::boo::Foo`.
2. Prefer using `T`, `U`, `V` etc for generic type param names, always start with `T`.
3. Use `thiserror` crate for errors.

### Testing

1. Use real objects where possible. When it's not possible, prefer crate-provided test utilities and/or in-memory fakes over mocks.
2. Use the test builder pattern described in this [post](https://jmmv.dev/2020/12/builder-pattern-for-tests.html).
3. Prefer extending an existing fake or test builder to support a use case over creating a new bespoke builder/fake. Good builders and fakes are general purpose, reusable across test suites, and mimic the behavior and APIs of the "real thing".
