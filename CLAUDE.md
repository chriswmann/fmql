# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build                        # build the library + `fmql` binary
cargo build --all-targets          # also build examples
cargo test                         # run unit tests + doctests
cargo test test_parse_select_all   # run a single test by name
cargo clippy --all-targets -- -D warnings   # lint (pedantic, set in Cargo.toml)
cargo run -- sql "SELECT * FROM . WHERE extension = 'rs'"   # run the CLI
cargo run --example sql_queries    # run an example end to end
```

CI (`.github/workflows/rust.yml`) runs `cargo fmt --check`, `cargo build --all-targets`,
`cargo test`, the `sql_queries` example and `cargo clippy --all-targets -- -D warnings`
on pushes to `main` and on pull requests targeting `main`. `Cargo.toml` enables
`clippy::pedantic`, so pedantic warnings fail CI too.

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
- `src/error.rs` — `FMQLError`, wrapping `ParserError` and `ExecutorError` via `#[from]`.
  `fmql::query()` (in `lib.rs`) parses and executes in one call and returns it.
- `src/output.rs` — `render_text()`, the CLI's `ls -l` style text output.
- `src/main.rs` — `clap` CLI with one subcommand, `sql`, and a `--format text|json` flag.
  Errors go to stderr with exit code 1; clap usage errors exit with 2.

Unit tests live in sibling `*_tests.rs` files pulled in via `#[cfg(test)] #[path = "..."] mod tests;`.
`tests/sql_pipeline.rs` exercises the parser, executor and CLI together against
temporary directories, including rejected updates that must not change files.

## Critical context before changing anything

- `parse_sql` uses sqlparser tokens and expression parsing with `FileDialect`.
  Its restricted statement grammar accepts `SELECT *` and permission updates,
  and requires complete input consumption. Unsupported syntax must return an
  error, never an absent or weakened condition.
- Both statements default to non-recursive traversal. `WITH RECURSIVE` enables
  recursion explicitly. The executor excludes the root directory from results.
- The parser expands `~` and normalises a leading dot in extension comparison
  literals. It validates attributes, literal types, dates, regexes and octal
  permissions before execution, even for empty directories or unused branches.
- `SELECT` supports `ORDER BY` (stable sort, nulls last ascending) and `LIMIT`;
  traversal visits entries by file name, so unordered output is deterministic.
  `GROUP BY`, `OFFSET`, projection and ownership/rename updates are rejected.
  The README documents the supported subset.
- The executor compiles a condition into a private `Predicate` before listing
  files, checking attributes, operand types and patterns in every branch, so a
  hand-built `FileQuery` fails with an `ExecutorError` instead of matching
  nothing. Update values are validated before any file changes. Accepted updates
  can still partially apply if a later filesystem operation fails.
- Updates skip symbolic links so they cannot change files outside the queried
  directory. Listing reads metadata through links, falling back to the link itself.
- `OVERHAUL_PLAN.md` retains the original review as history.

## Conventions

- Rust 2024 edition, formatted with `cargo fmt`. British English in prose, no em dashes.
- Per-module error enums via `thiserror`, with a module-local `Result<T>` alias.
