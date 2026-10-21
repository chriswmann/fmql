//! Regression tests for bugs found during code review.
//!
//! Each test targets a specific bug and should fail without its corresponding fix.

use crate::sql::ast::{
    ComparisonOperator, FileAttribute, FileAttributeUpdate, FileCondition, FileQuery, FileValue,
    UpdateValue,
};
use crate::sql::executor::execute_query;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

fn create_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    let mut f = File::create(&path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

// -----------------------------------------------------------------------
// Bug 1: list_files must NOT include the queried directory itself.
// -----------------------------------------------------------------------

#[test]
fn test_root_directory_excluded_from_results() {
    let dir = tempdir().unwrap();
    create_file(dir.path(), "a.txt", "hello");
    create_file(dir.path(), "b.txt", "world");

    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: false,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: None,
    };

    let results = execute_query(&query).unwrap();
    // None of the results should be the root directory itself.
    for r in &results {
        assert_ne!(
            r.path,
            dir.path().to_path_buf(),
            "Root directory should not appear in results"
        );
    }
    assert_eq!(results.len(), 2, "Expected exactly 2 files, no root dir");
}

// -----------------------------------------------------------------------
// Bug 2: non-recursive update must only touch top-level files.
// -----------------------------------------------------------------------

#[test]
#[cfg(unix)]
fn test_non_recursive_update_skips_subdirectory_files() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    create_file(dir.path(), "top.sh", "#!/bin/bash");
    create_file(dir.path(), "sub/nested.sh", "#!/bin/bash");

    let condition = FileCondition::Compare {
        attribute: FileAttribute::Extension,
        operator: ComparisonOperator::Eq,
        value: FileValue::String("sh".to_string()),
    };

    let query = FileQuery::Update {
        path: dir.path().to_path_buf(),
        recursive: false,
        updates: vec![FileAttributeUpdate {
            attribute: FileAttribute::Permissions,
            value: UpdateValue::Literal("755".to_string()),
        }],
        condition: Some(condition),
    };

    let updated = execute_query(&query).unwrap();

    // Only top.sh should be updated, not sub/nested.sh.
    assert_eq!(updated.len(), 1);
    assert_eq!(updated[0].name, "top.sh");

    // Verify nested.sh was NOT updated (it should still have default perms).
    let nested_meta = fs::metadata(dir.path().join("sub/nested.sh")).unwrap();
    let nested_mode = nested_meta.permissions().mode() & 0o777;
    assert_ne!(
        nested_mode, 0o755,
        "Nested file should not have been updated"
    );
}

// -----------------------------------------------------------------------
// Bug 3: Extension values -- parser and executor must agree (no leading dot).
// -----------------------------------------------------------------------

#[test]
fn test_extension_comparison_no_leading_dot() {
    let dir = tempdir().unwrap();
    create_file(dir.path(), "readme.txt", "content");
    create_file(dir.path(), "image.png", "img");

    // Simulate what the parser now emits: "txt" without dot.
    let condition = FileCondition::Compare {
        attribute: FileAttribute::Extension,
        operator: ComparisonOperator::Eq,
        value: FileValue::String("txt".to_string()),
    };

    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: false,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: Some(condition),
    };

    let results = execute_query(&query).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "readme.txt");
}

#[test]
fn test_extension_with_leading_dot_matches_nothing() {
    let dir = tempdir().unwrap();
    create_file(dir.path(), "readme.txt", "content");

    // Using ".txt" (with dot) should NOT match, proving the old parser value was wrong.
    let condition = FileCondition::Compare {
        attribute: FileAttribute::Extension,
        operator: ComparisonOperator::Eq,
        value: FileValue::String(".txt".to_string()),
    };

    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: false,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: Some(condition),
    };

    let results = execute_query(&query).unwrap();
    assert_eq!(results.len(), 0, "Leading dot should not match any file");
}

// -----------------------------------------------------------------------
// Bug 4: BETWEEN on modified with date strings must work.
// -----------------------------------------------------------------------

#[test]
fn test_between_modified_with_date_strings() {
    let dir = tempdir().unwrap();
    create_file(dir.path(), "recent.txt", "data");

    // The file was just created, so its mtime is "now".
    // Use a BETWEEN range that spans from the past to the far future.
    let condition = FileCondition::Between {
        attribute: FileAttribute::Modified,
        lower: FileValue::String("2020-01-01".to_string()),
        upper: FileValue::String("2099-12-31".to_string()),
    };

    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: false,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: Some(condition),
    };

    let results = execute_query(&query).unwrap();
    // Before the fix this would fail with a type error (DateTime vs String).
    assert_eq!(results.len(), 1);
}

#[test]
fn test_between_modified_excludes_outside_range() {
    let dir = tempdir().unwrap();
    create_file(dir.path(), "recent.txt", "data");

    // Use a range entirely in the past -- should match nothing.
    let condition = FileCondition::Between {
        attribute: FileAttribute::Modified,
        lower: FileValue::String("2000-01-01".to_string()),
        upper: FileValue::String("2001-01-01".to_string()),
    };

    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: false,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: Some(condition),
    };

    let results = execute_query(&query).unwrap();
    assert_eq!(
        results.len(),
        0,
        "No files should match a range in the past"
    );
}

// -----------------------------------------------------------------------
// Bug 6: LIKE pattern must treat regex metacharacters as literals.
// -----------------------------------------------------------------------

#[test]
fn test_like_does_not_interpret_dot_as_regex_wildcard() {
    let dir = tempdir().unwrap();
    create_file(dir.path(), "foo.bar", "a");
    create_file(dir.path(), "fooXbar", "b");

    // LIKE 'foo.bar' should match only "foo.bar", not "fooXbar".
    // Before the fix, the unescaped '.' acted as a regex wildcard.
    let condition = FileCondition::Like {
        attribute: FileAttribute::Name,
        pattern: "foo.bar".to_string(),
        case_sensitive: false,
    };

    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: false,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: Some(condition),
    };

    let results = execute_query(&query).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "foo.bar");
}

#[test]
fn test_like_percent_still_works_with_dots() {
    let dir = tempdir().unwrap();
    create_file(dir.path(), "config.ini", "a");
    create_file(dir.path(), "config.xml", "b");
    create_file(dir.path(), "readme.md", "c");

    // LIKE 'config%' should match files starting with "config".
    let condition = FileCondition::Like {
        attribute: FileAttribute::Name,
        pattern: "config%".to_string(),
        case_sensitive: false,
    };

    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: false,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: Some(condition),
    };

    let results = execute_query(&query).unwrap();
    assert_eq!(results.len(), 2);
    let names: Vec<_> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"config.ini"));
    assert!(names.contains(&"config.xml"));
}
