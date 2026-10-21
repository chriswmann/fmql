//! Grouped queries: `GROUP BY` and aggregate functions.

use std::cmp::Ordering;
use std::path::Path;

use serde::Serialize;

use super::{
    ExecutorError, FileResult, Predicate, Result, attribute_kind, attribute_value, matching_files,
    order_values,
};
use crate::sql::ast::{
    AggregateFunction, Column, FileAttribute, FileCondition, FileQuery, FileValue, GroupOrder,
};

/// The rows a grouped query returns.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Table {
    /// The columns, whose [`Column::label`]s are headings such as `count(*)`.
    pub columns: Vec<Column>,
    /// One row per group, each with one value per column.
    pub rows: Vec<Vec<FileValue>>,
}

/// Executes a [`FileQuery::Group`].
///
/// # Errors
///
/// Returns an error if the query is not a grouped one, if a column is neither
/// grouped nor an aggregate, if an aggregate does not suit its attribute, or if
/// listing the files fails.
///
/// # Examples
///
/// ```no_run
/// use fmql::sql::{execute_grouped, parse_sql};
///
/// let query = parse_sql("SELECT extension, COUNT(*) FROM . GROUP BY extension").unwrap();
/// let table = execute_grouped(&query).unwrap();
/// let labels: Vec<_> = table.columns.iter().map(|c| c.label()).collect();
/// assert_eq!(labels, ["extension", "count(*)"]);
/// ```
pub fn execute_grouped(query: &FileQuery) -> Result<Table> {
    let FileQuery::Group {
        path,
        recursive,
        columns,
        condition,
        group_by,
        order_by,
        limit,
    } = query
    else {
        return Err(ExecutorError::UnsupportedOperation(
            "only grouped queries return a table; use execute_query".to_string(),
        ));
    };
    group(
        path,
        *recursive,
        columns,
        condition.as_ref(),
        group_by,
        order_by,
        *limit,
    )
}

fn group(
    path: &Path,
    recursive: bool,
    columns: &[Column],
    condition: Option<&FileCondition>,
    group_by: &[FileAttribute],
    order_by: &[GroupOrder],
    limit: Option<usize>,
) -> Result<Table> {
    for attribute in group_by {
        attribute_kind(attribute)?;
    }
    for column in columns.iter().chain(order_by.iter().map(|key| &key.column)) {
        check_column(column, group_by)?;
    }
    let predicate = condition.map(Predicate::compile).transpose()?;
    let mut files = matching_files(path, recursive, predicate.as_ref())?;

    // Sorting by the group values puts each group's files next to each other.
    let key = |file: &FileResult| -> Vec<FileValue> {
        group_by.iter().map(|a| attribute_value(file, a)).collect()
    };
    files.sort_by(|a, b| compare_rows(&key(a), &key(b)));
    let mut groups: Vec<&[FileResult]> = Vec::new();
    let mut rest = files.as_slice();
    while let Some(first) = rest.first() {
        let first = key(first);
        let end = rest
            .iter()
            .position(|file| compare_rows(&key(file), &first).is_ne())
            .unwrap_or(rest.len());
        let (group, tail) = rest.split_at(end);
        groups.push(group);
        rest = tail;
    }
    // Without GROUP BY, all files form one group, even when there are none.
    if group_by.is_empty() && groups.is_empty() {
        groups.push(&[]);
    }

    if !order_by.is_empty() {
        let sort_key = |group: &[FileResult]| -> Vec<FileValue> {
            order_by
                .iter()
                .map(|key| column_value(&key.column, group))
                .collect()
        };
        // A stable sort keeps groups in key order between equal sort keys.
        groups.sort_by(|a, b| {
            sort_key(a)
                .iter()
                .zip(sort_key(b).iter())
                .zip(order_by)
                .map(|((a, b), key)| {
                    let ordering = order_values(a, b);
                    if key.descending {
                        ordering.reverse()
                    } else {
                        ordering
                    }
                })
                .find(|ordering| ordering.is_ne())
                .unwrap_or(Ordering::Equal)
        });
    }
    if let Some(limit) = limit {
        groups.truncate(limit);
    }

    Ok(Table {
        columns: columns.to_vec(),
        rows: groups
            .into_iter()
            .map(|group| columns.iter().map(|c| column_value(c, group)).collect())
            .collect(),
    })
}

/// Orders two rows of group values column by column.
fn compare_rows(left: &[FileValue], right: &[FileValue]) -> Ordering {
    left.iter()
        .zip(right)
        .map(|(l, r)| order_values(l, r))
        .find(|ordering| ordering.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// Rejects a column whose value could differ within a group, or an aggregate
/// that does not suit its attribute.
fn check_column(column: &Column, group_by: &[FileAttribute]) -> Result<()> {
    match column {
        Column::Attribute(attribute) if group_by.contains(attribute) => Ok(()),
        Column::Attribute(attribute) => Err(ExecutorError::UnsupportedOperation(format!(
            "{} must appear in GROUP BY or inside an aggregate",
            attribute.name()
        ))),
        Column::Aggregate {
            function: AggregateFunction::Count,
            attribute: None,
        } => Ok(()),
        Column::Aggregate {
            attribute: None, ..
        } => Err(ExecutorError::UnsupportedOperation(format!(
            "{} needs an attribute",
            column.label()
        ))),
        Column::Aggregate {
            function,
            attribute: Some(attribute),
        } => {
            attribute_kind(attribute)?;
            let numeric = matches!(function, AggregateFunction::Sum | AggregateFunction::Avg);
            // Adding permission bits means nothing; size is the only quantity.
            if numeric && *attribute != FileAttribute::Size {
                return Err(ExecutorError::TypeError(format!(
                    "{} needs the size attribute",
                    column.label()
                )));
            }
            Ok(())
        }
    }
}

/// Computes a column for one group. Every file in a group shares its
/// attribute values, so an attribute column reads the first file.
fn column_value(column: &Column, group: &[FileResult]) -> FileValue {
    let (function, attribute) = match column {
        Column::Attribute(attribute) => {
            return group
                .first()
                .map_or(FileValue::Null, |file| attribute_value(file, attribute));
        }
        Column::Aggregate {
            function,
            attribute: None,
        } => {
            debug_assert_eq!(*function, AggregateFunction::Count);
            return count(group.len());
        }
        Column::Aggregate {
            function,
            attribute: Some(attribute),
        } => (function, attribute),
    };
    let values: Vec<FileValue> = group
        .iter()
        .map(|file| attribute_value(file, attribute))
        .filter(|value| !matches!(value, FileValue::Null))
        .collect();
    match function {
        AggregateFunction::Count => count(values.len()),
        AggregateFunction::Sum => sum(&values).map_or(FileValue::Null, FileValue::Number),
        AggregateFunction::Avg => sum(&values).map_or(FileValue::Null, |total| {
            FileValue::Number(total / count_f64(values.len()))
        }),
        AggregateFunction::Min => values
            .into_iter()
            .min_by(order_values)
            .unwrap_or(FileValue::Null),
        AggregateFunction::Max => values
            .into_iter()
            .max_by(order_values)
            .unwrap_or(FileValue::Null),
    }
}

fn sum(values: &[FileValue]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(
        values
            .iter()
            .map(|value| match value {
                FileValue::Number(n) => *n,
                other => unreachable!("check_column limits SUM and AVG to size: {other:?}"),
            })
            .sum(),
    )
}

fn count(n: usize) -> FileValue {
    FileValue::Number(count_f64(n))
}

#[expect(clippy::cast_precision_loss, reason = "a group cannot hold 2^53 files")]
fn count_f64(n: usize) -> f64 {
    n as f64
}

#[cfg(test)]
#[path = "group_tests.rs"]
mod tests;
