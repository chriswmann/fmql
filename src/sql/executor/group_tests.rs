use std::fs;
use std::path::Path;

use tempfile::tempdir;

use crate::sql::ast::{AggregateFunction, Column, FileAttribute, FileQuery, FileValue, GroupOrder};
use crate::sql::executor::{ExecutorError, execute_grouped, execute_query};
use crate::sql::parse_sql;

fn table(dir: &Path, rest: &str) -> Vec<Vec<FileValue>> {
    let sql = rest.replace("$DIR", &dir.display().to_string());
    execute_grouped(&parse_sql(&sql).unwrap()).unwrap().rows
}

fn n(value: f64) -> FileValue {
    FileValue::Number(value)
}

fn s(value: &str) -> FileValue {
    FileValue::String(value.to_string())
}

fn fixture() -> tempfile::TempDir {
    let dir = tempdir().unwrap();
    for (name, size) in [("a.rs", 10), ("b.rs", 30), ("c.txt", 5), ("README", 1)] {
        fs::write(dir.path().join(name), vec![b'x'; size]).unwrap();
    }
    dir
}

#[test]
fn groups_are_counted_summed_and_ordered_by_key() {
    let dir = fixture();

    let rows = table(
        dir.path(),
        "SELECT extension, COUNT(*), SUM(size), AVG(size), MIN(name), MAX(size) \
         FROM $DIR GROUP BY extension",
    );

    assert_eq!(
        rows,
        [
            vec![s(""), n(1.0), n(1.0), n(1.0), s("README"), n(1.0)],
            vec![s("rs"), n(2.0), n(40.0), n(20.0), s("a.rs"), n(30.0)],
            vec![s("txt"), n(1.0), n(5.0), n(5.0), s("c.txt"), n(5.0)],
        ]
    );
}

#[test]
fn order_by_aggregate_and_limit_choose_rows() {
    let dir = fixture();

    let rows = table(
        dir.path(),
        "SELECT extension FROM $DIR GROUP BY extension ORDER BY SUM(size) DESC LIMIT 2",
    );

    assert_eq!(rows, [vec![s("rs")], vec![s("txt")]]);
}

#[test]
fn aggregates_without_group_by_return_one_row_even_when_empty() {
    let dir = tempdir().unwrap();

    let rows = table(
        dir.path(),
        "SELECT COUNT(*), SUM(size), MIN(modified) FROM $DIR",
    );
    let grouped = table(dir.path(), "SELECT extension FROM $DIR GROUP BY extension");

    assert_eq!(rows, [vec![n(0.0), FileValue::Null, FileValue::Null]]);
    assert_eq!(grouped, Vec::<Vec<FileValue>>::new());
}

#[test]
fn count_of_an_attribute_skips_nulls() {
    let dir = fixture();
    let rows = table(dir.path(), "SELECT COUNT(*), COUNT(owner) FROM $DIR");
    let named = nix::unistd::User::from_uid(nix::unistd::getuid())
        .ok()
        .flatten()
        .is_some();

    assert_eq!(rows, [vec![n(4.0), n(if named { 4.0 } else { 0.0 })]]);
}

#[test]
fn hand_built_group_queries_are_validated_before_listing() {
    let query = |columns, group_by| FileQuery::Group {
        path: "/definitely/missing/fmql".into(),
        recursive: false,
        columns,
        condition: None,
        group_by,
        order_by: vec![GroupOrder {
            column: Column::Attribute(FileAttribute::Name),
            descending: false,
        }],
        limit: None,
    };

    let ungrouped = execute_grouped(&query(
        vec![Column::Attribute(FileAttribute::Extension)],
        vec![FileAttribute::Extension],
    ))
    .unwrap_err();
    let sum_of_names = execute_grouped(&query(
        vec![Column::Aggregate {
            function: AggregateFunction::Sum,
            attribute: Some(FileAttribute::Name),
        }],
        vec![FileAttribute::Name],
    ))
    .unwrap_err();
    let files_from_group =
        execute_query(&query(Vec::new(), vec![FileAttribute::Name])).unwrap_err();

    assert!(
        matches!(ungrouped, ExecutorError::UnsupportedOperation(_)),
        "{ungrouped:?}"
    );
    assert!(
        matches!(sum_of_names, ExecutorError::TypeError(_)),
        "{sum_of_names:?}"
    );
    assert!(matches!(
        files_from_group,
        ExecutorError::UnsupportedOperation(_)
    ));
}
