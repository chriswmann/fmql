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
- `ORDER BY`, `GROUP BY`, projection, ownership/rename updates and unimplemented
  metadata are rejected. The README documents the supported subset.
- The public executor still discards evaluation errors in its filter. Callers
  constructing `FileQuery` directly bypass parser validation. Accepted updates
  can partially apply if a later filesystem operation fails; they are not atomic.
- `FMQLError` remains unused by the binary. Lint and formatting debt remains
  outside the parser work. `OVERHAUL_PLAN.md` retains the original review as history.

## Conventions

- Rust 2024 edition. British English in prose, no em dashes.
- Per-module error enums via `thiserror`, with a module-local `Result<T>` alias.
- `examples/group_by_updates.rs` uses `CONCAT`/`MONTH`/`DAY` SQL functions that no
  layer supports; parsing now returns an error. Do not treat it as a working reference.
