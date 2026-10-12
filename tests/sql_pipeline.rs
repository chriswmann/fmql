use fmql::sql::{execute_query, parse_sql};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn fixture() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (name, content) in [
        ("alpha.rs", "rust"),
        ("beta.txt", "text file"),
        ("it's.txt", "x"),
    ] {
        fs::write(dir.path().join(name), content).unwrap();
        fs::set_permissions(dir.path().join(name), fs::Permissions::from_mode(0o640)).unwrap();
    }
    fs::create_dir(dir.path().join("nested")).unwrap();
    fs::write(dir.path().join("nested/child.rs"), "child").unwrap();
    fs::set_permissions(
        dir.path().join("nested/child.rs"),
        fs::Permissions::from_mode(0o640),
    )
    .unwrap();
    dir
}

fn names(path: &Path, prefix: &str, condition: &str) -> Vec<String> {
    let sql = format!("{prefix}SELECT * FROM {} WHERE {condition}", path.display());
    let query = parse_sql(&sql).unwrap_or_else(|err| panic!("{sql}: {err}"));
    let mut names: Vec<_> = execute_query(&query)
        .unwrap()
        .into_iter()
        .map(|f| f.name)
        .collect();
    names.sort();
    names
}

#[test]
fn predicates_select_exact_files() {
    let dir = fixture();
    for (condition, expected) in [
        ("name = 'definitely_missing_fmql_file'", vec![]),
        ("extension = 'rs'", vec!["alpha.rs"]),
        ("extension = '.rs'", vec!["alpha.rs"]),
        ("name = 'it''s.txt'", vec!["it's.txt"]),
        (
            "name = 'alpha.rs' OR name = 'beta.txt' AND size > 5",
            vec!["alpha.rs", "beta.txt"],
        ),
        (
            "(name = 'alpha.rs' OR name = 'beta.txt') AND size > 5",
            vec!["beta.txt"],
        ),
        (
            "NOT (is_directory = true OR extension != 'txt')",
            vec!["beta.txt", "it's.txt"],
        ),
        (
            "size BETWEEN 4 AND 9 AND is_directory = false",
            vec!["alpha.rs", "beta.txt"],
        ),
        (
            "size NOT BETWEEN 4 AND 9 AND is_directory = false",
            vec!["it's.txt"],
        ),
        ("name like '%TXT'", vec!["beta.txt", "it's.txt"]),
        (
            "name NOT LIKE '%.txt' AND is_directory = false",
            vec!["alpha.rs"],
        ),
        ("REGEXP(name, '^a.*\\.rs$')", vec!["alpha.rs"]),
        ("name REGEXP '^b.*'", vec!["beta.txt"]),
        ("name = 'RECURSIVE WHERE FROM'", vec![]),
        (
            "permissions = 640",
            vec!["alpha.rs", "beta.txt", "it's.txt"],
        ),
        (
            "permissions = '0640' AND extension = 'rs'",
            vec!["alpha.rs"],
        ),
        ("permissions != 640 AND is_directory = false", vec![]),
        ("permissions = 644", vec![]),
    ] {
        assert_eq!(names(dir.path(), "", condition), expected, "{condition}");
    }
    assert_eq!(
        names(dir.path(), "WITH RECURSIVE ", "extension = 'rs'"),
        ["alpha.rs", "child.rs"]
    );
}

#[test]
fn permissions_update_respects_predicate_and_explicit_recursion() {
    for recursive in [false, true] {
        let dir = fixture();
        let prefix = if recursive { "WITH RECURSIVE " } else { "" };
        let sql = format!(
            "{prefix}UPDATE {} SET permissions = '600' WHERE extension = 'rs'",
            dir.path().display()
        );
        let files = execute_query(&parse_sql(&sql).unwrap()).unwrap();
        assert_eq!(files.len(), if recursive { 2 } else { 1 });
        assert_eq!(mode(&dir.path().join("alpha.rs")), 0o600);
        assert_eq!(mode(&dir.path().join("beta.txt")), 0o640);
        assert_eq!(
            mode(&dir.path().join("nested/child.rs")),
            if recursive { 0o600 } else { 0o640 }
        );
    }
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

#[test]
fn rejected_updates_fail_in_cli_without_changing_files() {
    let dir = fixture();
    for suffix in [
        "SET permissions = '755' WHERE name IN ('alpha.rs')",
        "SET permissions = '755' WHERE name LIKE '%.rs' AND unknown = 1",
        "SET permissions = '755', owner = 'admin'",
        "SET permissions = '755', permissions = '888'",
        "SET permissions = '755' WHERE REGEXP(name, '[')",
        "SET permissions = '755' WHERE size = 'four'",
        "SET permissions = '755' WHERE permissions != 33184",
        "SET permissions = '755' WHERE name = 'alpha.rs' RETURNING *",
        "SET permissions = '755'; SELECT * FROM .",
    ] {
        let sql = format!("UPDATE {} {suffix}", dir.path().display());
        let output = Command::new(env!("CARGO_BIN_EXE_fmql"))
            .args(["sql", &sql])
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted {sql}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Error parsing"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        for file in ["alpha.rs", "beta.txt", "it's.txt", "nested/child.rs"] {
            assert_eq!(
                mode(&dir.path().join(file)),
                0o640,
                "changed {file} for {sql}"
            );
        }
    }
}

#[test]
fn rejects_unsupported_or_incomplete_queries() {
    for sql in [
        "",
        "SELECT",
        "SELECT * FROM",
        "SELECT * FROM . WHERE",
        "SELECT * FROM . WHERE name =",
        "SELECT * FROM . WHERE extension = '.txt' AND size + 1 > 0",
        "SELECT * FROM . WHERE name LIKE '%x%' OR nonsense",
        "SELECT * FROM . WHERE created > '2026-01-01'",
        "SELECT * FROM WHERE name = 'x'",
        "UPDATE SET permissions = '600'",
        "SELECT * FROM . WHERE permissions = 888",
        "SELECT * FROM . WHERE permissions = 10000",
        "SELECT * FROM . WHERE permissions = -644",
        "SELECT * FROM . WHERE permissions = 6.4",
        "SELECT * FROM . WHERE permissions > 600",
        "SELECT * FROM . WHERE permissions BETWEEN 600 AND 700",
        "SELECT * FROM . WHERE modified > 'not a date'",
        "SELECT name FROM .",
        "SELECT DISTINCT * FROM .",
        "SELECT * FROM . ORDER BY size",
        "SELECT * FROM . LIMIT 1",
        "SELECT * FROM . GROUP BY extension",
        "SELECT * FROM . JOIN other ON name = name",
        "SELECT * FROM . AS files",
        "SELECT * FROM .; SELECT * FROM ..",
        "UPDATE . SET",
        "UPDATE . SET owner = 'bob'",
        "UPDATE . SET permissions = '888'",
        "UPDATE . SET permissions = '10000'",
        "UPDATE . SET permissions = '600', permissions = '700'",
        "SELECT * FROM . WHERE name LIKE 'x' ESCAPE '!'",
        "SELECT * FROM . WHERE REGEXP(name, 'a') FILTER (WHERE size > 1)",
        "SELECT * FROM . WHERE REGEXP(DISTINCT name, 'a')",
    ] {
        assert!(parse_sql(sql).is_err(), "accepted {sql}");
    }
}

#[test]
fn quoted_paths_keywords_comments_and_case_are_not_reinterpreted() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("WHERE RECURSIVE FROM SET");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("x.txt"), "x").unwrap();
    fs::create_dir(path.join("subdir")).unwrap();
    fs::write(path.join("subdir/x.txt"), "x").unwrap();
    let sql = format!(
        "/* test */ seLect * FrOm \"{}\" whEre NAME='x.txt'; -- done",
        path.display()
    );
    let result = execute_query(&parse_sql(&sql).unwrap()).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, path.join("x.txt"));
}

#[test]
fn modified_filters_use_supplied_dates_and_timestamps() {
    use chrono::{TimeZone, Utc};
    let dir = fixture();
    let timestamp = Utc
        .with_ymd_and_hms(2026, 7, 15, 12, 30, 0)
        .unwrap()
        .timestamp();
    filetime::set_file_mtime(
        dir.path().join("alpha.rs"),
        filetime::FileTime::from_unix_time(timestamp, 0),
    )
    .unwrap();
    for condition in [
        "name = 'alpha.rs' AND modified BETWEEN '2026-07-01' AND '2026-08-01'",
        "name = 'alpha.rs' AND modified = '2026-07-15T13:30:00+01:00'",
        "name = 'alpha.rs' AND modified NOT BETWEEN '2026-08-01' AND '2026-09-01'",
    ] {
        assert_eq!(names(dir.path(), "", condition), ["alpha.rs"]);
    }
    assert_eq!(
        names(
            dir.path(),
            "",
            "name = 'alpha.rs' AND modified < '2026-07-15'"
        ),
        Vec::<String>::new()
    );
}

#[test]
fn invalid_queries_are_rejected_on_empty_directories_and_unreachable_branches() {
    let dir = tempfile::tempdir().unwrap();
    for condition in [
        "name = 'missing' AND size = 'invalid'",
        "is_directory = true OR REGEXP(name, '[')",
        "modified > '2026-02-30'",
    ] {
        for statement in ["SELECT * FROM", "UPDATE"] {
            let update = if statement == "UPDATE" {
                " SET permissions = '755'"
            } else {
                ""
            };
            let sql = format!(
                "{statement} {}{update} WHERE {condition}",
                dir.path().display()
            );
            assert!(parse_sql(&sql).is_err(), "{sql}");
        }
    }
}

#[test]
fn quoted_paths_preserve_punctuation_and_embedded_quotes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("semi; quote\" and ' apostrophe");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("only.txt"), "contents").unwrap();
    let escaped = path.to_str().unwrap().replace('"', "\"\"");
    let query = parse_sql(&format!(
        "SELECT * FROM \"{escaped}\" WHERE name = 'only.txt'"
    ))
    .unwrap();
    let result = execute_query(&query).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, path.join("only.txt"));
}

#[test]
fn cli_missing_name_is_an_empty_successful_result() {
    let dir = fixture();
    let sql = format!(
        "SELECT * FROM {} WHERE name = 'definitely_missing_fmql_file'",
        dir.path().display()
    );
    let output = Command::new(env!("CARGO_BIN_EXE_fmql"))
        .args(["sql", &sql, "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!([])
    );
}

#[test]
fn permission_filters_limit_updates_to_the_matching_mode() {
    let dir = fixture();
    fs::set_permissions(
        dir.path().join("beta.txt"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    let sql = format!(
        "UPDATE {} SET permissions = '400' WHERE permissions != 640 AND is_directory = false",
        dir.path().display()
    );
    let files = execute_query(&parse_sql(&sql).unwrap()).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(mode(&dir.path().join("beta.txt")), 0o400);
    assert_eq!(mode(&dir.path().join("alpha.rs")), 0o640);
    assert_eq!(mode(&dir.path().join("it's.txt")), 0o640);
}

#[test]
fn like_wildcards_match_newlines_in_names() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("new\nline.txt"), "x").unwrap();
    assert_eq!(
        names(dir.path(), "", "name LIKE '%.txt'"),
        ["new\nline.txt"]
    );
    assert_eq!(
        names(dir.path(), "", "name NOT LIKE 'new_line%'"),
        Vec::<String>::new()
    );
}

#[test]
fn keyword_named_directories_need_no_quoting() {
    let root = tempfile::tempdir().unwrap();
    for name in ["data", "cache", "user"] {
        fs::create_dir(root.path().join(name)).unwrap();
        fs::write(root.path().join(name).join("x.txt"), "x").unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_fmql"))
            .current_dir(root.path())
            .args(["sql", &format!("SELECT * FROM {name} WHERE name = 'x.txt'")])
            .args(["--format", "json"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let files: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(files.as_array().unwrap().len(), 1, "{name}");
    }
}

#[test]
fn updates_never_change_files_through_symbolic_links() {
    use std::os::unix::fs::symlink;
    for prefix in ["", "WITH RECURSIVE "] {
        let dir = fixture();
        let outside = fixture();
        symlink(outside.path().join("alpha.rs"), dir.path().join("link.rs")).unwrap();
        symlink(outside.path().join("nested"), dir.path().join("linked_dir")).unwrap();
        symlink(outside.path().join("missing"), dir.path().join("broken.rs")).unwrap();
        let outside_dir_mode = mode(&outside.path().join("nested"));

        let sql = format!(
            "{prefix}UPDATE {} SET permissions = '600' WHERE is_directory = false OR name = 'linked_dir'",
            dir.path().display()
        );
        let updated: Vec<_> = execute_query(&parse_sql(&sql).unwrap())
            .unwrap()
            .into_iter()
            .map(|f| f.name)
            .collect();
        assert!(updated.contains(&"alpha.rs".to_owned()), "{sql}");
        for link in ["link.rs", "linked_dir", "broken.rs"] {
            assert!(!updated.contains(&link.to_owned()), "updated {link}");
        }
        assert_eq!(mode(&dir.path().join("alpha.rs")), 0o600);
        assert_eq!(mode(&outside.path().join("alpha.rs")), 0o640);
        assert_eq!(mode(&outside.path().join("nested")), outside_dir_mode);
        assert_eq!(mode(&outside.path().join("nested/child.rs")), 0o640);
    }
}

#[test]
fn broken_symbolic_links_are_listed_without_failing_the_query() {
    let dir = fixture();
    std::os::unix::fs::symlink(dir.path().join("missing"), dir.path().join("broken.rs")).unwrap();
    assert_eq!(
        names(dir.path(), "", "extension = 'rs'"),
        ["alpha.rs", "broken.rs"]
    );
}
