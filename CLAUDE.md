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

CI (`.github/workflows/rust.yml`) runs `cargo fmt --check`, `cargo build --all-targets --locked`,
`cargo test --locked`, the `sql_queries` example and `cargo clippy --all-targets -- -D warnings`
on pushes to `main` and on pull requests targeting `main`. `Cargo.toml` enables
`clippy::pedantic`, so pedantic warnings fail CI too. `Cargo.lock` is committed;
run `cargo update` deliberately and commit the result.

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
- `src/output.rs` — `render_text()` (`ls -l` style lines for `SELECT *`) and the
  projected `render_columns()` / `columns_json()`.
- `src/main.rs` — `clap` CLI with one subcommand, `sql`, and a `--format text|json` flag.
  `--sudo[=PROGRAM]` execs `PROGRAM <fmql> --home=<caller's home> <args less --sudo>`
  unless already root; the hidden `--home` feeds `parse_sql_with_home`, so `~`
  keeps meaning the caller's home under root.
  Errors go to stderr with exit code 1; clap usage errors exit with 2.

Unit tests live in sibling `*_tests.rs` files pulled in via `#[cfg(test)] #[path = "..."] mod tests;`.
`tests/sql_pipeline.rs` exercises the parser, executor and CLI together against
temporary directories, including rejected updates that must not change files.

## Critical context before changing anything

- `parse_sql` uses sqlparser tokens and expression parsing with `FileDialect`.
  Its restricted statement grammar accepts `SELECT` (`*` or named columns) and
  `UPDATE` assignments to `permissions`, `owner` and `name` (a `NameExpr`),
  and requires complete input consumption. Unsupported syntax must return an
  error, never an absent or weakened condition.
- Both statements default to non-recursive traversal. `WITH RECURSIVE` enables
  recursion explicitly. The executor excludes the root directory from results.
- The parser expands `~` and normalises a leading dot in extension comparison
  literals. It validates attributes, literal types, dates, regexes and octal
  permissions before execution, even for empty directories or unused branches.
- `SELECT` supports `ORDER BY` (stable sort, nulls last ascending) and `LIMIT`;
  traversal visits entries by file name, so unordered output is deterministic.
  `SELECT a, b` projects columns; the executor still returns whole `FileResult`s
  and `output.rs` (`render_columns`, `columns_json`) applies the projection.
  `GROUP BY` or an aggregate in the select list makes `FileQuery::Group`, executed
  by `execute_grouped` (`src/sql/executor/group.rs`) into a `Table`, not files;
  `fmql::run` returns either. `HAVING`, `OFFSET` and select-list expressions are rejected.
  The README documents the supported subset.
- The executor compiles a condition into a private `Predicate` before listing
  files, checking attributes, operand types and patterns in every branch, so a
  hand-built `FileQuery` fails with an `ExecutorError` instead of matching
  nothing. Update values are validated before any file changes. `Changes::plan`
  validates assignments (resolving owner names); `apply_changes` applies each
  file's `Op`s (owner, then mode) and, when one fails, restores every touched
  file's owner then mode (`UpdateFailed`), naming any it cannot restore
  (`RollbackFailed`). It takes the operation as a closure so tests can inject
  failures; `apply_op` is the real one. `SET name = <NameExpr>` renames run
  after attribute changes, deepest first; `Changes::renames` validates names
  and rejects existing or shared destinations before anything changes; at apply
  time `rename_no_replace` (`renameat2(RENAME_NOREPLACE)` on Linux glibc, a
  check-then-rename elsewhere) refuses to replace anything that appeared since.
- Updates skip symbolic links so they cannot change files outside the queried
  directory, and `apply_op` (`lchown`, `chmod_no_follow`) refuses to follow a
  link that replaced a file after planning. Listing reads metadata through
  links, falling back to the link itself.
- `OVERHAUL_PLAN.md` retains the original review as history.

## Conventions

- Rust 2024 edition, formatted with `cargo fmt`. British English in prose, no em dashes.
- Per-module error enums via `thiserror`, with a module-local `Result<T>` alias.
