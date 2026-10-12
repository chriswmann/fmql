# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                        # build the library + `fmql` binary
cargo build --all-targets          # also build examples (CI does NOT do this)
cargo test                         # run unit tests + doctests
cargo test test_parse_select_all   # run a single test by name
cargo clippy --all-targets         # lint (CI runs `cargo clippy` without -D warnings)
cargo run -- sql "SELECT * FROM . WHERE extension = 'rs'"   # run the CLI
cargo run --example sql_queries    # run an example end to end
```

CI (`.github/workflows/rust.yml`) only runs `cargo build`, `cargo test`, `cargo clippy`
on `main`. Clippy warnings do not fail CI, so a green tick does not mean lint-clean.

## Architecture

A single pipeline turns a SQL-ish string into file-system results:

```
sql string ──parse_sql()──▶ FileQuery (AST) ──execute_query()──▶ Vec<FileResult>
            src/sql/parser.rs            src/sql/executor.rs
```

- `src/sql/ast.rs` — the intermediate representation: `FileQuery` (`Select`/`Update`),
  `FileCondition` (recursive enum: `Compare`/`And`/`Or`/`Not`/`Like`/`Between`/`Regexp`),
  `FileAttribute`, `FileValue`. This is the contract between parser and executor.
- `src/sql/parser.rs` — `parse_sql()`. Defines its own `ParserError`.
- `src/sql/executor.rs` — `execute_query()` walks the filesystem with `walkdir`,
  builds `FileResult`s, and evaluates conditions. Defines its own `ExecutorError`.
- `src/sql/dialect.rs` — `FileDialect`, a custom `sqlparser` dialect allowing `~`, `/`,
  `.` etc. in identifiers.
- `src/error.rs` — `FMQLError`. The crate's two real error types live in `parser.rs`
  and `executor.rs` instead; there is no unified error type.
- `src/main.rs` — `clap` CLI with one subcommand, `sql`.

Unit tests live in sibling `*_tests.rs` files pulled in via `#[cfg(test)] #[path = "..."] mod tests;`.

## Critical context before changing anything

The crate does not work as the README describes. Treat the README as a wishlist.

- **`parse_sql` is a string-matching stub, not a parser.** It pattern-matches against
  the exact strings in `parser_tests.rs` (hardcoded regexes, dates, extensions). It
  does NOT use `sqlparser` or `FileDialect` at all. Any query outside the test fixtures
  silently returns the wrong result (usually "no condition" → all files). The passing
  tests are the mock validating its own hardcoded answers.
- `FileDialect` and the entire `sqlparser` dependency are dead code (build emits a
  `never constructed` warning). `FMQLError` is also dead.
- The AST has no representation for `ORDER BY` or `GROUP BY`; the README's "sorting"
  and "grouping" features do not exist. `display.rs` and `file.rs` were deleted (still
  visible in `target/package/fmql-0.2.x/`); table output and the `ls` mode are gone.
- Known executor bugs: `list_files` includes the root directory itself in results;
  `execute_update` always recurses regardless of intent; `BETWEEN` on `modified` fails
  because date `FileValue::String` literals are never parsed into `FileValue::DateTime`;
  `evaluate_condition` errors are swallowed by `.unwrap_or(false)` in the filter.
- Extension values are inconsistent: `path.extension()` yields no leading dot (`rs`),
  but the stub parser emits values with a dot (`.txt`). Check which side you are on.

## Conventions

- Rust 2024 edition. British English in prose, no em dashes.
- Per-module error enums via `thiserror`, with a module-local `Result<T>` alias.
- `examples/group_by_updates.rs` uses `CONCAT`/`MONTH`/`DAY` SQL functions that no
  layer supports; it parses to a no-op. Do not treat it as a working reference.
