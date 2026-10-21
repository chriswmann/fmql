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
        "SET permissions = '755', size = 1",
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
        "SELECT * FROM . WHERE created > 'yesterday'",
        "SELECT * FROM WHERE name = 'x'",
        "UPDATE SET permissions = '600'",
        "SELECT * FROM . WHERE permissions = 888",
        "SELECT * FROM . WHERE permissions = 10000",
        "SELECT * FROM . WHERE permissions = -644",
        "SELECT * FROM . WHERE permissions = 6.4",
        "SELECT * FROM . WHERE permissions > 600",
        "SELECT * FROM . WHERE permissions BETWEEN 600 AND 700",
        "SELECT * FROM . WHERE modified > 'not a date'",
        "SELECT FROM .",
        "SELECT name, FROM .",
        "SELECT name, name FROM .",
        "SELECT *, name FROM .",
        "SELECT name, * FROM .",
        "SELECT nonsense FROM .",
        "SELECT size + 1 FROM .",
        "SELECT name AS n FROM .",
        "UPDATE . SET permissions = '600' RETURNING name",
        "SELECT DISTINCT * FROM .",
        "SELECT * FROM . ORDER BY",
        "SELECT * FROM . ORDER BY size,",
        "SELECT * FROM . ORDER BY unknown",
        "SELECT * FROM . ORDER BY size + 1",
        "SELECT * FROM . ORDER BY size NULLS FIRST",
        "SELECT * FROM . LIMIT -1",
        "SELECT * FROM . LIMIT 'x'",
        "SELECT * FROM . LIMIT 1 OFFSET 1",
        "SELECT * FROM . LIMIT 1 ORDER BY size",
        "UPDATE . SET permissions = '600' ORDER BY size",
        "UPDATE . SET permissions = '600' LIMIT 1",
        "SELECT * FROM . GROUP BY extension",
        "SELECT * FROM . JOIN other ON name = name",
        "SELECT * FROM . AS files",
        "SELECT * FROM .; SELECT * FROM ..",
        "UPDATE . SET",
        "UPDATE . SET owner = 'bob', owner = 'eve'",
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

fn fmql(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_fmql"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn cli_text_output_lists_one_entry_per_line() {
    let dir = fixture();
    let sql = format!(
        "SELECT * FROM {} WHERE extension = 'rs'",
        dir.path().display()
    );
    let output = fmql(&["sql", &sql]);
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 1, "{stdout}");
    assert!(lines[0].starts_with("-rw-r-----  4  "), "{stdout}");
    assert!(
        lines[0].ends_with(&format!("  {}", dir.path().join("alpha.rs").display())),
        "{stdout}"
    );
}

#[test]
fn cli_rejects_unknown_output_formats() {
    let output = fmql(&["sql", "SELECT * FROM .", "--format", "yaml"]);
    assert_eq!(output.status.code(), Some(2));
    // Check the names clap lists, not its wording, which may change in 4.x.
    let stderr = String::from_utf8_lossy(&output.stderr);
    for word in ["yaml", "text", "json"] {
        assert!(stderr.contains(word), "{stderr}");
    }
}

#[test]
fn cli_reports_execution_errors_separately_from_parse_errors() {
    let dir = fixture();
    let sql = format!("SELECT * FROM {}", dir.path().join("missing").display());
    let output = fmql(&["sql", &sql]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).starts_with("Error executing query: "),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
}

#[test]
fn query_runs_parse_and_execute_together() {
    let dir = fixture();
    let sql = format!(
        "WITH RECURSIVE SELECT * FROM {} WHERE extension = 'rs'",
        dir.path().display()
    );
    let mut names: Vec<_> = fmql::query(&sql)
        .unwrap()
        .into_iter()
        .map(|f| f.name)
        .collect();
    names.sort();
    assert_eq!(names, ["alpha.rs", "child.rs"]);
    assert!(matches!(
        fmql::query("SELECT nonsense FROM ."),
        Err(fmql::FMQLError::Parse(_))
    ));
}

#[test]
fn metadata_attributes_filter_through_sql() {
    let dir = fixture();
    std::os::unix::fs::symlink(dir.path().join("alpha.rs"), dir.path().join("link.rs")).unwrap();
    let me = nix::unistd::User::from_uid(nix::unistd::getuid())
        .ok()
        .flatten()
        .map(|user| user.name);

    assert_eq!(names(dir.path(), "", "is_symlink = true"), ["link.rs"]);
    assert_eq!(
        names(dir.path(), "", "extension = 'rs' AND NOT is_symlink"),
        ["alpha.rs"]
    );
    // Without a user database entry for our uid, owners are null.
    if let Some(me) = me {
        assert_eq!(
            names(
                dir.path(),
                "",
                &format!("owner = '{me}' AND name LIKE 'b%'")
            ),
            ["beta.txt"]
        );
    }
    assert_eq!(
        names(dir.path(), "", "owner LIKE 'no-such-user-%'"),
        Vec::<String>::new()
    );
    // Access times are optional too; an unavailable one is null and never matches.
    let has_atime = fs::metadata(dir.path().join("beta.txt"))
        .unwrap()
        .accessed()
        .is_ok();
    assert_eq!(
        names(
            dir.path(),
            "",
            "accessed > '2000-01-01' AND name = 'beta.txt'"
        ),
        if has_atime { vec!["beta.txt"] } else { vec![] }
    );
    // Birth times are optional, so only check that the query is accepted.
    parse_sql("SELECT * FROM . WHERE created BETWEEN '2000-01-01' AND '2100-01-01'").unwrap();
}

fn ordered_names(sql: &str) -> Vec<String> {
    fmql::query(sql)
        .unwrap_or_else(|err| panic!("{sql}: {err}"))
        .into_iter()
        .map(|f| f.name)
        .collect()
}

#[test]
fn order_by_and_limit_shape_select_results() {
    let dir = fixture();
    let from = format!("SELECT * FROM {}", dir.path().display());

    // Without ORDER BY, entries come back in name order.
    assert_eq!(
        ordered_names(&from),
        ["alpha.rs", "beta.txt", "it's.txt", "nested"]
    );
    assert_eq!(
        ordered_names(&format!("{from} WHERE NOT is_directory ORDER BY size DESC")),
        ["beta.txt", "alpha.rs", "it's.txt"]
    );
    assert_eq!(
        ordered_names(&format!(
            "{from} ORDER BY is_directory DESC, extension, name DESC"
        )),
        ["nested", "alpha.rs", "it's.txt", "beta.txt"]
    );
    assert_eq!(
        ordered_names(&format!(
            "{from} WHERE extension = 'txt' ORDER BY size ASC LIMIT 1;"
        )),
        ["it's.txt"]
    );
    assert_eq!(
        ordered_names(&format!("{from} LIMIT 2")),
        ["alpha.rs", "beta.txt"]
    );
    assert_eq!(
        ordered_names(&format!("{from} LIMIT 0")),
        Vec::<String>::new()
    );
    assert_eq!(
        ordered_names(&format!(
            "WITH RECURSIVE {from} WHERE extension = 'rs' ORDER BY path DESC"
        )),
        ["child.rs", "alpha.rs"]
    );
}

#[test]
fn cli_projects_selected_attributes_in_order() {
    let dir = fixture();
    let sql = format!(
        "SELECT size, name, permissions FROM {} WHERE NOT is_directory ORDER BY size",
        dir.path().display()
    );

    let text = fmql(&["sql", &sql]);
    assert!(text.status.success());
    assert_eq!(
        String::from_utf8(text.stdout).unwrap(),
        "1  it's.txt  640\n4  alpha.rs  640\n9  beta.txt  640\n"
    );

    let json = fmql(&["sql", &sql, "--format", "json"]);
    assert!(json.status.success());
    let rows: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let first = rows[0].as_object().unwrap();
    assert_eq!(
        first.keys().collect::<Vec<_>>(),
        ["size", "name", "permissions"]
    );
    assert_eq!(rows[2]["name"], "beta.txt");
    assert_eq!(rows[2]["size"], 9);
    assert_eq!(rows[2]["permissions"], 0o100_640);
}

#[test]
fn mode_matches_permissions_as_ls_shows_them() {
    let dir = fixture();
    fs::set_permissions(
        dir.path().join("alpha.rs"),
        fs::Permissions::from_mode(0o750),
    )
    .unwrap();
    fs::set_permissions(dir.path().join("nested"), fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink(dir.path().join("beta.txt"), dir.path().join("link")).unwrap();

    assert_eq!(
        names(dir.path(), "", "mode = '-rw-r-----'"),
        ["beta.txt", "it's.txt"]
    );
    assert_eq!(
        names(dir.path(), "", "mode LIKE '___x%'"),
        ["alpha.rs", "nested"]
    );
    assert_eq!(names(dir.path(), "", "mode LIKE 'd%'"), ["nested"]);
    assert_eq!(names(dir.path(), "", "mode LIKE 'l%'"), ["link"]);
    assert_eq!(
        names(dir.path(), "", "mode REGEXP '^-.{6}r'"),
        Vec::<String>::new()
    );
    assert_eq!(
        names(dir.path(), "", "NOT mode REGEXP 'x' AND NOT is_symlink"),
        ["beta.txt", "it's.txt"]
    );

    let sql = format!(
        "SELECT mode, name FROM {} WHERE name = 'alpha.rs'",
        dir.path().display()
    );
    let output = fmql(&["sql", &sql]);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "-rwxr-x---  alpha.rs\n"
    );
    assert!(parse_sql("SELECT * FROM . WHERE mode = 640").is_err());

    // A socket is not a regular file, so `-%` must not select it.
    let _socket = std::os::unix::net::UnixListener::bind(dir.path().join("sock")).unwrap();
    assert_eq!(names(dir.path(), "", "mode LIKE 's%'"), ["sock"]);
    assert!(!names(dir.path(), "", "mode LIKE '-%'").contains(&"sock".to_string()));
}

#[test]
fn owner_updates_resolve_users_before_changing_anything() {
    let dir = fixture();
    let sql = format!(
        "UPDATE {} SET permissions = '600', owner = 'no-such-user-fmql' WHERE name = 'alpha.rs'",
        dir.path().display()
    );
    let output = fmql(&["sql", &sql]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Unknown user: no-such-user-fmql"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(mode(&dir.path().join("alpha.rs")), 0o640);
}

#[test]
fn owner_and_permissions_update_together() {
    let Some(me) = nix::unistd::User::from_uid(nix::unistd::getuid())
        .ok()
        .flatten()
        .map(|user| user.name)
    else {
        eprintln!("skipping: the current uid has no user database entry");
        return;
    };
    let dir = fixture();
    // Without privileges a file can only be given to its current owner, which
    // still exercises the whole path through chown.
    let sql = format!(
        "UPDATE {} SET owner = '{me}', permissions = '604' WHERE extension = 'txt'",
        dir.path().display()
    );
    let updated = fmql::query(&sql).unwrap();
    let mut summary: Vec<_> = updated
        .iter()
        .map(|f| (f.name.as_str(), f.owner.as_deref(), f.permissions & 0o7777))
        .collect();
    summary.sort_unstable();
    assert_eq!(
        summary,
        [
            ("beta.txt", Some(me.as_str()), 0o604),
            ("it's.txt", Some(me.as_str()), 0o604)
        ]
    );
    assert_eq!(mode(&dir.path().join("beta.txt")), 0o604);
    assert_eq!(mode(&dir.path().join("alpha.rs")), 0o640);
}

#[test]
fn cli_renames_matching_files() {
    let dir = fixture();
    let sql = format!(
        "WITH RECURSIVE UPDATE {} SET name = LOWER(REPLACE(name, '.rs', '.RS.bak')) \
         WHERE extension = 'rs'",
        dir.path().display()
    );
    let output = fmql(&["sql", &sql, "--format", "json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let updated: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mut names: Vec<_> = updated
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap().to_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["alpha.rs.bak", "child.rs.bak"]);
    assert!(dir.path().join("alpha.rs.bak").exists());
    assert!(dir.path().join("nested/child.rs.bak").exists());
    assert!(!dir.path().join("alpha.rs").exists());
    // Renaming onto an existing file fails without moving anything.
    let sql = format!(
        "UPDATE {} SET name = 'beta.txt' WHERE name = 'it''s.txt'",
        dir.path().display()
    );
    let output = fmql(&["sql", &sql]);
    assert_eq!(output.status.code(), Some(1));
    assert!(dir.path().join("it's.txt").exists());
}
