#[cfg(test)]
use crate::sql::ast::{
    ComparisonOperator, FileAttribute, FileAttributeUpdate, FileCondition, FileQuery, FileValue,
};
use crate::sql::executor::{ExecutorError, execute_query};
use std::fs::{self, File};
use std::io::Result as IoResult;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use tempfile::{TempDir, tempdir};

fn setup_test_directory() -> TempDir {
    let dir = tempdir().unwrap();

    // Create some test files
    let _ = create_test_file(dir.path(), "file1.txt", "Hello, world!");
    let _ = create_test_file(dir.path(), "file2.txt", "This is a test.");
    let _ = create_test_file(dir.path(), "config.ini", "[section]\nkey=value");
    let _ = create_test_file(dir.path(), "script.sh", "#!/bin/bash\necho 'Hello'");

    // Create a subdirectory
    let subdir_path = dir.path().join("subdir");
    fs::create_dir(&subdir_path).unwrap();
    let _ = create_test_file(dir.path(), "subdir/file3.txt", "Nested file.");
    let _ = create_test_file(dir.path(), "subdir/config.xml", "<config></config>");

    // Make script.sh executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script_path = dir.path().join("script.sh");
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    dir
}

fn create_test_file(directory: &Path, filename: &str, content: &str) -> IoResult<PathBuf> {
    let path = directory.join(filename);
    let mut file = File::create(&path)?;
    file.write_all(content.as_bytes())?;
    Ok(path)
}

#[test]
fn test_execute_select_all() {
    let dir = setup_test_directory();
    let _dir_path = dir.path().to_string_lossy().to_string();

    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: false,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: None,
    };

    let results = execute_query(&query).unwrap();

    // There should be 5 entries (4 files + subdir, root directory excluded)
    assert_eq!(results.len(), 5);

    // Verify some basic properties
    let files: Vec<_> = results
        .iter()
        .filter(|f| !f.is_directory)
        .map(|f| f.name.clone())
        .collect();

    assert!(files.contains(&"file1.txt".to_string()));
    assert!(files.contains(&"file2.txt".to_string()));
    assert!(files.contains(&"config.ini".to_string()));
    assert!(files.contains(&"script.sh".to_string()));
}

#[test]
fn test_execute_select_recursive() {
    let dir = setup_test_directory();

    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: true,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: None,
    };

    let results = execute_query(&query).unwrap();

    // There should be 7 entries (4 files + subdir + 2 subdir files, root excluded)
    assert_eq!(results.len(), 7);

    let all_files: Vec<_> = results
        .iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();

    // Check for the presence of subdirectory files
    assert!(all_files.iter().any(|p| p.contains("subdir/file3.txt")));
    assert!(all_files.iter().any(|p| p.contains("subdir/config.xml")));
}

#[test]
fn test_execute_select_with_extension_filter() {
    let dir = setup_test_directory();

    let condition = FileCondition::Compare {
        attribute: FileAttribute::Extension,
        operator: ComparisonOperator::Eq,
        value: FileValue::String("txt".to_string()),
    };

    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: true,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: Some(condition),
    };

    let results = execute_query(&query).unwrap();

    // There should be 3 .txt files
    assert_eq!(results.len(), 3);

    // Verify they all have .txt extension
    for file in &results {
        assert_eq!(file.extension.as_ref().unwrap(), "txt");
    }
}

#[test]
fn test_execute_select_with_name_like() {
    let dir = setup_test_directory();

    let condition = FileCondition::Like {
        attribute: FileAttribute::Name,
        pattern: "%config%".to_string(),
        case_sensitive: false,
    };

    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: true,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: Some(condition),
    };

    let results = execute_query(&query).unwrap();

    // There should be 2 files with "config" in the name
    assert_eq!(results.len(), 2);

    let names: Vec<_> = results.iter().map(|f| f.name.clone()).collect();
    assert!(names.contains(&"config.ini".to_string()));
    assert!(names.contains(&"config.xml".to_string()));
}

#[test]
fn test_execute_select_with_size_comparison() {
    let dir = setup_test_directory();

    let condition = FileCondition::Compare {
        attribute: FileAttribute::Size,
        operator: ComparisonOperator::Gt,
        value: FileValue::Number(10.0),
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

    // Check that we only got files larger than 10 bytes
    for file in &results {
        assert!(file.size > 10);
    }
}

#[test]
fn test_execute_select_with_complex_condition() {
    let dir = setup_test_directory();

    // Find .txt files with size > 5 bytes
    let txt_condition = FileCondition::Compare {
        attribute: FileAttribute::Extension,
        operator: ComparisonOperator::Eq,
        value: FileValue::String("txt".to_string()),
    };

    let size_condition = FileCondition::Compare {
        attribute: FileAttribute::Size,
        operator: ComparisonOperator::Gt,
        value: FileValue::Number(5.0),
    };

    let combined_condition = FileCondition::And(Box::new(txt_condition), Box::new(size_condition));

    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: true,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: Some(combined_condition),
    };

    let results = execute_query(&query).unwrap();

    // Verify results
    for file in &results {
        assert_eq!(file.extension.as_ref().unwrap(), "txt");
        assert!(file.size > 5);
    }
}

#[test]
fn test_execute_select_with_regexp() {
    let dir = setup_test_directory();

    let condition = FileCondition::Regexp {
        attribute: FileAttribute::Name,
        pattern: "^file[0-9]\\.txt$".to_string(),
    };

    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: true,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: Some(condition),
    };

    let results = execute_query(&query).unwrap();

    // There should be 3 files matching the pattern (file1.txt, file2.txt, and file3.txt)
    assert_eq!(results.len(), 3);

    let names: Vec<_> = results.iter().map(|f| f.name.clone()).collect();
    assert!(names.contains(&"file1.txt".to_string()));
    assert!(names.contains(&"file2.txt".to_string()));
    assert!(names.contains(&"file3.txt".to_string()));
}

#[test]
#[cfg(unix)]
fn test_execute_update_permissions() {
    let dir = setup_test_directory();

    let condition = FileCondition::Compare {
        attribute: FileAttribute::Extension,
        operator: ComparisonOperator::Eq,
        value: FileValue::String("txt".to_string()),
    };

    let updates = vec![FileAttributeUpdate {
        attribute: FileAttribute::Permissions,
        value: "644".to_string(),
    }];

    let query = FileQuery::Update {
        path: dir.path().to_path_buf(),
        recursive: true,
        updates,
        condition: Some(condition),
    };

    let updated_files = execute_query(&query).unwrap();

    // Check that permissions were updated
    for file in &updated_files {
        assert_eq!(file.permissions & 0o777, 0o644);
    }
}

fn select(dir: &Path, condition: FileCondition) -> FileQuery {
    FileQuery::Select {
        path: dir.to_path_buf(),
        recursive: false,
        attributes: vec![FileAttribute::All],
        order_by: Vec::new(),
        limit: None,
        condition: Some(condition),
    }
}

#[test]
fn test_type_mismatch_is_an_error_not_a_non_match() {
    let dir = setup_test_directory();
    let condition = FileCondition::Compare {
        attribute: FileAttribute::Size,
        operator: ComparisonOperator::Eq,
        value: FileValue::String("big".to_string()),
    };

    let err = execute_query(&select(dir.path(), condition)).unwrap_err();

    assert!(matches!(err, ExecutorError::TypeError(_)), "{err:?}");
}

#[test]
fn test_unsupported_attribute_is_an_error() {
    let dir = setup_test_directory();
    let condition = FileCondition::Compare {
        attribute: FileAttribute::All,
        operator: ComparisonOperator::Eq,
        value: FileValue::String("x".to_string()),
    };

    let err = execute_query(&select(dir.path(), condition)).unwrap_err();

    assert!(
        matches!(err, ExecutorError::UnsupportedAttribute(_)),
        "{err:?}"
    );
}

#[test]
fn test_like_on_non_string_attribute_is_an_error() {
    let dir = setup_test_directory();
    let condition = FileCondition::Like {
        attribute: FileAttribute::Size,
        pattern: "1%".to_string(),
        case_sensitive: false,
    };

    let err = execute_query(&select(dir.path(), condition)).unwrap_err();

    assert!(matches!(err, ExecutorError::TypeError(_)), "{err:?}");
}

#[test]
fn test_invalid_regex_fails_before_listing_an_empty_directory() {
    let dir = tempdir().unwrap();
    let condition = FileCondition::Regexp {
        attribute: FileAttribute::Name,
        pattern: "(unclosed".to_string(),
    };

    let err = execute_query(&select(dir.path(), condition)).unwrap_err();

    assert!(matches!(err, ExecutorError::InvalidRegex(_)), "{err:?}");
}

#[test]
fn test_invalid_regex_in_unevaluated_branch_is_still_an_error() {
    let dir = setup_test_directory();
    let always_true = FileCondition::Compare {
        attribute: FileAttribute::Size,
        operator: ComparisonOperator::GtEq,
        value: FileValue::Number(0.0),
    };
    let bad_regex = FileCondition::Regexp {
        attribute: FileAttribute::Name,
        pattern: "[".to_string(),
    };
    let condition = FileCondition::Or(Box::new(always_true), Box::new(bad_regex));

    let err = execute_query(&select(dir.path(), condition)).unwrap_err();

    assert!(matches!(err, ExecutorError::InvalidRegex(_)), "{err:?}");
}

#[test]
fn test_case_sensitive_like_distinguishes_case() {
    let dir = setup_test_directory();
    let like = |case_sensitive| FileCondition::Like {
        attribute: FileAttribute::Name,
        pattern: "FILE%".to_string(),
        case_sensitive,
    };

    let insensitive = execute_query(&select(dir.path(), like(false))).unwrap();
    let sensitive = execute_query(&select(dir.path(), like(true))).unwrap();

    assert_eq!(insensitive.len(), 2);
    assert!(sensitive.is_empty());
}

#[test]
#[cfg(unix)]
fn test_update_with_failing_condition_changes_nothing() {
    use std::os::unix::fs::PermissionsExt;

    let dir = setup_test_directory();
    let target = dir.path().join("file1.txt");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    let condition = FileCondition::Or(
        Box::new(FileCondition::Compare {
            attribute: FileAttribute::Name,
            operator: ComparisonOperator::Eq,
            value: FileValue::String("file1.txt".to_string()),
        }),
        Box::new(FileCondition::Compare {
            attribute: FileAttribute::Size,
            operator: ComparisonOperator::Eq,
            value: FileValue::Boolean(true),
        }),
    );
    let query = FileQuery::Update {
        path: dir.path().to_path_buf(),
        recursive: false,
        updates: vec![FileAttributeUpdate {
            attribute: FileAttribute::Permissions,
            value: "777".to_string(),
        }],
        condition: Some(condition),
    };

    let err = execute_query(&query).unwrap_err();

    assert!(matches!(err, ExecutorError::TypeError(_)), "{err:?}");
    let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn test_unparsable_date_literal_is_an_error() {
    let dir = setup_test_directory();
    let condition = FileCondition::Compare {
        attribute: FileAttribute::Modified,
        operator: ComparisonOperator::Gt,
        value: FileValue::String("last tuesday".to_string()),
    };

    let err = execute_query(&select(dir.path(), condition)).unwrap_err();

    assert!(matches!(err, ExecutorError::TypeError(_)), "{err:?}");
}

#[test]
fn test_ordering_a_boolean_is_unsupported() {
    let dir = setup_test_directory();
    let condition = FileCondition::Compare {
        attribute: FileAttribute::IsDirectory,
        operator: ComparisonOperator::Lt,
        value: FileValue::Boolean(true),
    };

    let err = execute_query(&select(dir.path(), condition)).unwrap_err();

    assert!(
        matches!(err, ExecutorError::UnsupportedOperation(_)),
        "{err:?}"
    );
}

#[test]
fn test_null_compares_equal_only_to_null() {
    let dir = setup_test_directory();
    let owner_is = |operator, value| FileCondition::Compare {
        attribute: FileAttribute::Owner,
        operator,
        value,
    };

    let null = execute_query(&select(
        dir.path(),
        owner_is(ComparisonOperator::Eq, FileValue::Null),
    ));
    let not_null = execute_query(&select(
        dir.path(),
        owner_is(ComparisonOperator::NotEq, FileValue::Null),
    ));

    // The fixture's owner is null only if the user database lacks our uid.
    let expected_nulls = if current_user().is_some() { 0 } else { 5 };
    assert_eq!(null.unwrap().len(), expected_nulls);
    assert_eq!(not_null.unwrap().len(), 5 - expected_nulls);
}

/// The current user's name, or `None` in containers whose uid has no entry.
fn current_user() -> Option<String> {
    nix::unistd::User::from_uid(nix::unistd::getuid())
        .ok()
        .flatten()
        .map(|user| user.name)
}

#[test]
#[cfg(unix)]
fn test_owner_is_the_current_user() {
    let Some(me) = current_user() else {
        eprintln!("skipping: the current uid has no user database entry");
        return;
    };
    let dir = setup_test_directory();
    let condition = FileCondition::Compare {
        attribute: FileAttribute::Owner,
        operator: ComparisonOperator::Eq,
        value: FileValue::String(me.clone()),
    };

    let results = execute_query(&select(dir.path(), condition)).unwrap();

    assert_eq!(results.len(), 5);
    assert!(
        results
            .iter()
            .all(|f| f.owner.as_deref() == Some(me.as_str()))
    );
}

#[test]
#[cfg(unix)]
fn test_is_symlink_describes_the_link_and_its_target() {
    let dir = setup_test_directory();
    std::os::unix::fs::symlink(dir.path().join("subdir"), dir.path().join("link")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("gone"), dir.path().join("dangling")).unwrap();
    let condition = FileCondition::Compare {
        attribute: FileAttribute::IsSymlink,
        operator: ComparisonOperator::Eq,
        value: FileValue::Boolean(true),
    };

    let mut links = execute_query(&select(dir.path(), condition)).unwrap();
    links.sort_by(|a, b| a.name.cmp(&b.name));

    let summary: Vec<_> = links
        .iter()
        .map(|f| (f.name.as_str(), f.is_symlink, f.is_directory))
        .collect();
    assert_eq!(summary, [("dangling", true, false), ("link", true, true)]);
}

#[test]
fn test_created_and_accessed_are_comparable_times() {
    let dir = setup_test_directory();
    for attribute in [FileAttribute::Created, FileAttribute::Accessed] {
        let condition = FileCondition::Compare {
            attribute: attribute.clone(),
            operator: ComparisonOperator::Gt,
            value: FileValue::String("2000-01-01".to_string()),
        };

        let results = execute_query(&select(dir.path(), condition)).unwrap();

        // A filesystem without birth times yields nulls, which match nothing.
        let known = execute_query(&select(
            dir.path(),
            FileCondition::Compare {
                attribute,
                operator: ComparisonOperator::NotEq,
                value: FileValue::Null,
            },
        ))
        .unwrap();
        assert_eq!(results.len(), known.len());
    }
}

#[test]
#[cfg(unix)]
fn test_invalid_update_value_changes_nothing() {
    use std::os::unix::fs::PermissionsExt;

    let dir = setup_test_directory();
    let target = dir.path().join("file1.txt");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    let query = FileQuery::Update {
        path: dir.path().to_path_buf(),
        recursive: false,
        updates: vec![
            FileAttributeUpdate {
                attribute: FileAttribute::Permissions,
                value: "644".to_string(),
            },
            FileAttributeUpdate {
                attribute: FileAttribute::Permissions,
                value: "17777".to_string(),
            },
        ],
        condition: None,
    };

    let err = execute_query(&query).unwrap_err();

    assert!(matches!(err, ExecutorError::TypeError(_)), "{err:?}");
    let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn test_update_without_changes_still_validates_its_condition() {
    let dir = setup_test_directory();
    let query = FileQuery::Update {
        path: dir.path().to_path_buf(),
        recursive: false,
        updates: Vec::new(),
        condition: Some(FileCondition::Regexp {
            attribute: FileAttribute::Name,
            pattern: "[".to_string(),
        }),
    };

    let err = execute_query(&query).unwrap_err();

    assert!(matches!(err, ExecutorError::InvalidRegex(_)), "{err:?}");
}

#[test]
fn test_nulls_sort_after_values() {
    use super::order_values;
    use std::cmp::Ordering;

    let value = FileValue::Number(1.0);
    assert_eq!(order_values(&FileValue::Null, &value), Ordering::Greater);
    assert_eq!(order_values(&value, &FileValue::Null), Ordering::Less);
    assert_eq!(
        order_values(&FileValue::Null, &FileValue::Null),
        Ordering::Equal
    );
}

#[test]
fn test_order_by_star_is_rejected() {
    let dir = setup_test_directory();
    let query = FileQuery::Select {
        path: dir.path().to_path_buf(),
        recursive: false,
        attributes: vec![FileAttribute::All],
        condition: None,
        order_by: vec![crate::sql::ast::OrderBy {
            attribute: FileAttribute::All,
            descending: false,
        }],
        limit: None,
    };

    let err = execute_query(&query).unwrap_err();

    assert!(
        matches!(err, ExecutorError::UnsupportedAttribute(_)),
        "{err:?}"
    );
}

#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

#[cfg(unix)]
fn three_files(dir: &Path) -> Vec<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    ["a", "b", "c"]
        .iter()
        .map(|name| {
            let path = create_test_file(dir, name, name).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            path
        })
        .collect()
}

#[test]
#[cfg(unix)]
fn test_failed_update_rolls_back_earlier_changes() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let paths = three_files(dir.path());
    let failing = paths[2].clone();

    let err = super::apply_mode(&paths, 0o755, |path, mode| {
        if path == failing && mode == 0o755 {
            return Err(std::io::Error::other("injected"));
        }
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    })
    .unwrap_err();

    assert!(
        matches!(&err, ExecutorError::UpdateFailed { path, .. } if *path == failing),
        "{err:?}"
    );
    assert!(err.to_string().contains("rolled back"), "{err}");
    for path in &paths {
        assert_eq!(mode_of(path), 0o600, "{}", path.display());
    }
}

#[test]
#[cfg(unix)]
fn test_failed_rollback_names_the_files_left_changed() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let paths = three_files(dir.path());
    let (stuck, failing) = (paths[0].clone(), paths[2].clone());

    let err = super::apply_mode(&paths, 0o755, |path, mode| {
        if (path == failing && mode == 0o755) || (path == stuck && mode == 0o600) {
            return Err(std::io::Error::other("injected"));
        }
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    })
    .unwrap_err();

    let ExecutorError::RollbackFailed { unrestored, .. } = &err else {
        panic!("{err:?}");
    };
    assert_eq!(unrestored, std::slice::from_ref(&stuck));
    assert!(
        err.to_string().contains(&stuck.display().to_string()),
        "{err}"
    );
    assert_eq!(mode_of(&stuck), 0o755);
    assert_eq!(mode_of(&paths[1]), 0o600);
    assert_eq!(mode_of(&failing), 0o600);
}

#[test]
#[cfg(unix)]
fn test_successful_update_reports_every_changed_file() {
    let dir = tempdir().unwrap();
    let paths = three_files(dir.path());
    let query = FileQuery::Update {
        path: dir.path().to_path_buf(),
        recursive: false,
        updates: vec![FileAttributeUpdate {
            attribute: FileAttribute::Permissions,
            value: "640".to_string(),
        }],
        condition: None,
    };

    let updated = execute_query(&query).unwrap();

    assert_eq!(updated.len(), 3);
    assert!(updated.iter().all(|f| f.permissions & 0o7777 == 0o640));
    // The file-type bits survive alongside the new permission bits.
    assert!(
        updated
            .iter()
            .all(|f| f.permissions & 0o170_000 == 0o100_000)
    );
    for path in &paths {
        assert_eq!(mode_of(path), 0o640);
    }
}

#[test]
#[cfg(unix)]
fn test_update_query_surfaces_a_rolled_back_failure() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let paths = three_files(dir.path());
    let failing = paths[1].clone();
    let updates = [FileAttributeUpdate {
        attribute: FileAttribute::Permissions,
        value: "755".to_string(),
    }];

    let err = super::update_with(dir.path(), false, &updates, None, |path, mode| {
        if path == failing && mode == 0o755 {
            return Err(std::io::Error::other("injected"));
        }
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
    })
    .unwrap_err();

    assert!(
        matches!(&err, ExecutorError::UpdateFailed { path, .. } if *path == failing),
        "{err:?}"
    );
    for path in &paths {
        assert_eq!(mode_of(path), 0o600, "{}", path.display());
    }
}
