# FMQL Overhaul Plan

A code review and staged plan to turn FMQL from a test-passing mock into a
working tool. Written 2026-05-22 against commit `f5037c3` (v0.3.0).

## Progress as of 2026-10-21

The review below is historical; every numbered item is now resolved.

- 2026-10-12 (#1): the stub parser was replaced with sqlparser tokenisation and
  expression parsing behind a restricted grammar (C1-C3, H1-H3, H5, M5, M6).
- #2, #3: rustfmt, `clippy::pedantic` and examples are enforced in CI, unused
  dependencies are gone, and `examples/group_by_updates.rs` is deleted (M8, L2).
- #4: conditions compile to a checked predicate before traversal, so evaluation
  errors surface instead of being swallowed (H4).
- #5: `FMQLError` wraps the parser and executor errors, `fmql::query` runs a
  query end to end, and `--format` is a `ValueEnum` with `ls -l` style text (M4, M7).
- #6: `owner`, `created`, `accessed` and `is_symlink` are populated and queryable (M3).
- #7, #8: `ORDER BY`, `LIMIT` and projected columns are implemented (M1).
- #9: a permission update that fails part way rolls back earlier changes.
- #10: `Cargo.lock` is committed and CI builds with `--locked`.

- #12: a `mode` attribute holds the `ls -l` string, so permissions can be
  matched with `LIKE` and `REGEXP` (M2).

- #13: `UPDATE ... SET owner = 'name'`, alone or with `permissions`; rollback
  restores both owner and mode.

Still out of scope: `GROUP BY` and rename updates.

## 1. Review summary

### Severity: critical

| # | Issue | Location |
|---|-------|----------|
| C1 | `parse_sql` is a string-matching stub. It recognises only the exact fixtures in `parser_tests.rs` (hardcoded regex `^server_[0-9]+\.log$`, dates `2025-01-01`/`2025-03-31`, extensions `.txt`/`.bin`). Any other query silently produces the wrong AST, usually `condition: None` so every file matches. | `src/sql/parser.rs` |
| C2 | `sqlparser` and `FileDialect` are never used. The dependency, the dialect module, and its doctests are all dead weight. The README claims the opposite. | `src/sql/dialect.rs`, `Cargo.toml` |
| C3 | Tests give false confidence. `parser_tests.rs` checks the stub against its own hardcoded outputs; `executor_tests.rs` `test_execute_select_all` asserts `len() == 6`, baking in the "root dir included" bug. Green CI proves nothing. | `src/sql/*_tests.rs` |

### Severity: high

| # | Issue | Location |
|---|-------|----------|
| H1 | `list_files` yields the walked root directory itself as a result. | `executor.rs:191` |
| H2 | `execute_update` always walks recursively (`list_files(path, true)`); `FileQuery::Update` has no `recursive` field, so the caller cannot scope it. A bad `WHERE` recursively chmods a tree. | `executor.rs:136` |
| H3 | `BETWEEN`/comparisons on `modified` are broken. `modified` resolves to `FileValue::DateTime` but date literals stay `FileValue::String`; `compare_values` falls through to a `TypeError`. Date strings are never parsed. | `parser.rs:297`, `executor.rs:313` |
| H4 | `evaluate_condition` errors are discarded because the filter uses `unwrap_or(false)`. Type errors and bad regexes vanish instead of surfacing. | `executor.rs:121,140` |
| H5 | Extension dot inconsistency: `path.extension()` gives `rs`; the stub parser emits `.txt`. Equality silently never matches across the two conventions. | `executor.rs:228`, `parser.rs:327` |

### Severity: medium

| # | Issue | Location |
|---|-------|----------|
| M1 | No `ORDER BY` / `GROUP BY` in the AST; README's sorting and grouping do not exist. `display.rs`/`file.rs` and the `ls` mode were removed. | `ast.rs`, README |
| M2 | `permissions` resolves to a `Number`; README's `permissions LIKE '%x%'` cannot work. No rwx-string representation. | `executor.rs:352` |
| M3 | `created`/`accessed` are `FileAttribute` variants but unsupported in `get_attribute_value` (returns `UnsupportedAttribute`). `is_symlink`, `owner` are never populated. | `executor.rs:343` |
| M4 | Three parallel error types (`FMQLError`, `ParserError`, `ExecutorError`); `FMQLError` is unused dead code. No crate-wide error. | `error.rs` |
| M5 | `extract_path_from_sql` splits on whitespace, so any path containing a space breaks. Quoted paths unsupported. | `parser.rs:199` |
| M6 | `parse_sql`'s `WITH RECURSIVE` branch is unreachable for recursion detection (the `SELECT` branch already sets `recursive` via `.contains("RECURSIVE")`); confused control flow. | `parser.rs:132-166` |
| M7 | `--format` is a free `String`; anything but `json` silently means text. Should be a `clap` `ValueEnum`. | `main.rs:25` |
| M8 | Float `==` on `FileValue::Number`; clippy `clippy::float_cmp`. ~34 clippy warnings overall (mostly `uninlined_format_args`). | `executor.rs:389` |

### Severity: low

- L1 `error.rs` has a dangling doc comment with no variant and an empty `# Examples`.
- L2 `examples/group_by_updates.rs` uses `CONCAT`/`MONTH`/`DAY`/`HOUR` and `SET name`/`SET path`, none supported; it parses to a silent no-op.
- L3 README is satire-heavy and materially inaccurate (claims sqlparser use, sorting, grouping, two output formats). Needs an honest "current status" section.
- L4 `target/package/` is committed-adjacent clutter from `cargo package`; confirm `.gitignore` covers `target/`.

## 2. Overhaul strategy

The core decision: **replace the stub parser with a real one built on `sqlparser` +
`FileDialect`**, which already exist and are designed for exactly this. Everything
else follows from having a trustworthy AST.

Work in small, independently reviewable PRs. Each phase ends green.

### Phase 0 - Honest baseline (no behaviour change)
- Rewrite the test suite first so it would *fail* against the stub: parser tests
  asserting real structure for queries NOT in the current fixtures; executor tests
  that exclude the root directory. This makes the breakage visible and gives the
  rewrite a real target.
- Add `#![deny(warnings)]` is too aggressive; instead make CI run
  `cargo clippy --all-targets -- -D warnings` and `cargo build --all-targets` so
  examples and lints actually gate.
- Rewrite the README's feature list to match reality; add a "Status" section.

### Phase 1 - Real parser (fixes C1, C2, M5, M6, H5)
- `parse_sql` parses with `Parser::parse_sql(&FileDialect::default(), sql)`.
- Walk the `sqlparser` `Statement::Query` / `Statement::Update` into `FileQuery`.
- Map `sqlparser` `Expr` (`BinaryOp`, `Like`, `Between`, `UnaryOp` NOT, function
  calls for `REGEXP`) onto `FileCondition`.
- Normalise extension literals (strip a leading dot) at parse time so executor and
  parser agree on one convention.
- Decide and document: is `~` expansion the parser's job or the executor's? Keep it
  in one place.
- Delete `FMQLError` or make it the crate-wide error (see Phase 4).

### Phase 2 - Executor correctness (fixes H1-H4, M2, M3)
- `list_files`: skip the root entry (or `min_depth(1)`).
- Add `recursive` to `FileQuery::Update`; thread it through the parser.
- Parse date literals into `DateTime<Utc>` so `modified`/`created` comparisons work;
  support `created`/`accessed` in `get_attribute_value`.
- Stop swallowing errors: change the filter to collect `Result`s and fail the query
  on a genuine evaluation error (or warn once), rather than `unwrap_or(false)`.
- Represent permissions so `LIKE '%x%'` is meaningful, or document that permissions
  are numeric-only and update the README.
- Populate `owner` / `is_symlink` or remove the attributes until implemented.

### Phase 3 - Feature parity with the README (M1)
Either implement or cut. Recommended: implement `ORDER BY` (cheap - sort the
`Vec<FileResult>`), defer `GROUP BY`. Add `OrderBy { attribute, descending }` to the
AST. Remove sorting/grouping/table-output claims from the README until shipped.

### Phase 4 - Polish
- Unify errors: one `FMQLError` with `#[from] ParserError`/`#[from] ExecutorError`,
  or keep module errors and add a crate-level wrapper for `main`.
- `--format` becomes a `clap` `ValueEnum`.
- Fix all clippy warnings; keep CI gating on `-D warnings`.
- Fix or delete `examples/group_by_updates.rs`.
- Add an integration test that runs real `parse_sql` → `execute_query` against a
  `tempfile` tree, asserting concrete file sets.

## 3. Suggested PR order

1. Phase 0 (failing tests + CI gating + honest README)
2. Phase 1 (real parser) - the biggest PR; this is where the value is
3. Phase 2 (executor fixes)
4. Phase 3 (`ORDER BY`)
5. Phase 4 (polish)

Phases 1 and 2 each restore a large amount of advertised behaviour; everything
before Phase 1 is groundwork to make the rewrite safe and the rewrite after it
verifiable.
