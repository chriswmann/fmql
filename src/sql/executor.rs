//! Executor for SQL-like file management commands.
//!
//! This module provides functionality to execute parsed SQL-like commands
//! on the file system, such as querying files or updating file attributes.

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Serialize;
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

use crate::sql::ast::{ComparisonOperator, FileAttribute, FileCondition, FileQuery, FileValue};

/// Errors that can occur during query execution.
#[derive(Error, Debug)]
pub enum ExecutorError {
    /// Error from std::io operations.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Error when a file attribute is not supported.
    #[error("Unsupported file attribute: {0}")]
    UnsupportedAttribute(String),

    /// Error when an operation is not supported.
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    /// Error when a regular expression is invalid.
    #[error("Invalid regular expression: {0}")]
    InvalidRegex(#[from] regex::Error),

    /// Error when a value is of the wrong type.
    #[error("Type error: {0}")]
    TypeError(String),
}

/// Result type for executor operations.
pub type Result<T> = std::result::Result<T, ExecutorError>;

/// Represents a file that matches a query.
#[derive(Debug, Clone, Serialize)]
pub struct FileResult {
    /// The file path.
    pub path: PathBuf,
    /// The file name.
    pub name: String,
    /// The file size in bytes.
    pub size: u64,
    /// Whether the file is a directory.
    pub is_directory: bool,
    /// The file extension, if any.
    pub extension: Option<String>,
    /// The file permissions.
    pub permissions: u32,
    /// The file modification time.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub modified: DateTime<Utc>,
    /// The file owner, if available.
    pub owner: Option<String>,
}

/// Executes a parsed FileQuery.
///
/// # Arguments
///
/// * `query` - The parsed FileQuery to execute.
///
/// # Returns
///
/// A Result containing a vector of FileResult instances or an ExecutorError.
///
/// # Examples
///
/// Example of using execute_query to process a parsed query:
///
/// ```no_run
/// use fmql::sql::{parse_sql, execute_query};
///
/// // Parse a query
/// let query = parse_sql("SELECT * FROM /var/log WHERE name LIKE '%.log'").unwrap();
///
/// // Execute the query to get matching files
/// let results = execute_query(&query).unwrap();
///
/// // Process the results
/// for file in results {
///     println!("{}: {} bytes", file.name, file.size);
/// }
/// ```
pub fn execute_query(query: &FileQuery) -> Result<Vec<FileResult>> {
    match query {
        FileQuery::Select {
            path,
            recursive,
            attributes,
            condition,
        } => execute_select(path, *recursive, attributes, condition.as_ref()),
        FileQuery::Update {
            path,
            recursive,
            updates,
            condition,
        } => execute_update(path, *recursive, updates, condition.as_ref()),
    }
}

/// Executes a SELECT query.
fn execute_select(
    path: &Path,
    recursive: bool,
    _attributes: &[FileAttribute],
    condition: Option<&FileCondition>,
) -> Result<Vec<FileResult>> {
    let files = list_files(path, recursive)?;
    let filtered_files = if let Some(cond) = condition {
        files
            .into_iter()
            .filter(|file| evaluate_condition(file, cond).unwrap_or(false))
            .collect()
    } else {
        files
    };

    Ok(filtered_files)
}

/// Executes an UPDATE query.
fn execute_update(
    path: &Path,
    recursive: bool,
    updates: &[crate::sql::ast::FileAttributeUpdate],
    condition: Option<&FileCondition>,
) -> Result<Vec<FileResult>> {
    let files = list_files(path, recursive)?;
    let filtered_files = if let Some(cond) = condition {
        files
            .into_iter()
            .filter(|file| evaluate_condition(file, cond).unwrap_or(false))
            .collect()
    } else {
        files
    };

    let mut updated_files = Vec::new();

    for file in filtered_files {
        // Setting permissions acts on a link's target, which may lie outside
        // the queried directory, so links are never updated.
        if fs::symlink_metadata(&file.path)?.file_type().is_symlink() {
            continue;
        }

        let mut file_updated = false;

        for update in updates {
            match update.attribute {
                FileAttribute::Permissions => {
                    let perms = u32::from_str_radix(&update.value, 8).map_err(|_| {
                        ExecutorError::TypeError(format!(
                            "Invalid permissions value: {}",
                            update.value
                        ))
                    })?;

                    fs::set_permissions(&file.path, Permissions::from_mode(perms))?;
                    file_updated = true;
                }
                FileAttribute::Owner => {
                    // Note: Changing ownership requires platform-specific code and often root privileges
                    // This is a simplified example
                    return Err(ExecutorError::UnsupportedOperation(
                        "Changing file ownership is not implemented".to_string(),
                    ));
                }
                _ => {
                    return Err(ExecutorError::UnsupportedAttribute(format!(
                        "Cannot update attribute: {:?}",
                        update.attribute
                    )));
                }
            }
        }

        if file_updated {
            // Re-read the file info to get updated attributes
            let updated_file = create_file_result(&file.path)?;
            updated_files.push(updated_file);
        }
    }

    Ok(updated_files)
}

/// Lists files in a directory, optionally recursively.
fn list_files(dir_path: &Path, recursive: bool) -> Result<Vec<FileResult>> {
    let mut results = Vec::new();

    let walker = if recursive {
        WalkDir::new(dir_path).follow_links(false).into_iter()
    } else {
        WalkDir::new(dir_path)
            .max_depth(1)
            .follow_links(false)
            .into_iter()
    };

    for entry in walker {
        let entry = entry.map_err(|e| {
            ExecutorError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to read directory entry: {}", e),
            ))
        })?;

        // Skip the root directory itself -- we only want its children.
        if entry.path() == dir_path && entry.file_type().is_dir() {
            continue;
        }

        let file_result = create_file_result(entry.path())?;
        results.push(file_result);
    }

    Ok(results)
}

/// Creates a FileResult from a path.
fn create_file_result(path: &Path) -> Result<FileResult> {
    // Describe a link by its target, falling back to the link itself when the
    // target is missing so one broken link cannot fail the whole query.
    let metadata = fs::metadata(path).or_else(|_| fs::symlink_metadata(path))?;

    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_string());

    let modified = metadata
        .modified()
        .map(DateTime::<Utc>::from)
        .unwrap_or_else(|_| Utc::now());

    let permissions = metadata.permissions().mode();

    // Getting the owner requires platform-specific code
    // This is a simplified version
    let owner = None;

    Ok(FileResult {
        path: path.to_path_buf(),
        name,
        size: metadata.len(),
        is_directory: metadata.is_dir(),
        extension,
        permissions,
        modified,
        owner,
    })
}

/// Evaluates a condition against a file.
fn evaluate_condition(file: &FileResult, condition: &FileCondition) -> Result<bool> {
    match condition {
        FileCondition::Compare {
            attribute,
            operator,
            value,
        } => {
            let file_value = get_attribute_value(file, attribute)?;
            compare_values(&file_value, operator, value)
        }
        FileCondition::And(left, right) => {
            let left_result = evaluate_condition(file, left)?;
            if !left_result {
                return Ok(false);
            }
            evaluate_condition(file, right)
        }
        FileCondition::Or(left, right) => {
            let left_result = evaluate_condition(file, left)?;
            if left_result {
                return Ok(true);
            }
            evaluate_condition(file, right)
        }
        FileCondition::Not(inner) => {
            let inner_result = evaluate_condition(file, inner)?;
            Ok(!inner_result)
        }
        FileCondition::Like {
            attribute,
            pattern,
            case_sensitive,
        } => {
            let file_value = get_attribute_value(file, attribute)?;

            match file_value {
                FileValue::String(s) => {
                    let file_str = if *case_sensitive { s } else { s.to_lowercase() };
                    let pattern_str = if *case_sensitive {
                        pattern.clone()
                    } else {
                        pattern.to_lowercase()
                    };

                    // Convert SQL LIKE pattern to regex.
                    // Replace SQL wildcards with unique sentinels, escape
                    // regex metacharacters in the rest, then substitute
                    // sentinels with regex equivalents.
                    let with_sentinels = pattern_str
                        .replace('%', "\x00PERCENT\x00")
                        .replace('_', "\x00UNDER\x00");
                    let escaped = regex::escape(&with_sentinels);
                    let regex_pattern = escaped
                        .replace("\x00PERCENT\x00", ".*")
                        .replace("\x00UNDER\x00", ".");

                    // `(?s)` lets wildcards match a newline within a file name.
                    let regex = Regex::new(&format!("(?s)^{}$", regex_pattern))?;
                    Ok(regex.is_match(&file_str))
                }
                _ => Err(ExecutorError::TypeError(format!(
                    "LIKE can only be used with string attributes, got {:?}",
                    file_value
                ))),
            }
        }
        FileCondition::Between {
            attribute,
            lower,
            upper,
        } => {
            let file_value = get_attribute_value(file, attribute)?;

            let greater_than_lower = compare_values(&file_value, &ComparisonOperator::GtEq, lower)?;
            let less_than_upper = compare_values(&file_value, &ComparisonOperator::LtEq, upper)?;

            Ok(greater_than_lower && less_than_upper)
        }
        FileCondition::Regexp { attribute, pattern } => {
            let file_value = get_attribute_value(file, attribute)?;

            match file_value {
                FileValue::String(s) => {
                    let regex = Regex::new(pattern)?;
                    Ok(regex.is_match(&s))
                }
                _ => Err(ExecutorError::TypeError(format!(
                    "REGEXP can only be used with string attributes, got {:?}",
                    file_value
                ))),
            }
        }
    }
}

/// Gets the value of a file attribute.
fn get_attribute_value(file: &FileResult, attribute: &FileAttribute) -> Result<FileValue> {
    match attribute {
        FileAttribute::Name => Ok(FileValue::String(file.name.clone())),
        FileAttribute::Path => Ok(FileValue::String(file.path.to_string_lossy().to_string())),
        FileAttribute::Size => Ok(FileValue::Number(file.size as f64)),
        FileAttribute::Extension => Ok(FileValue::String(
            file.extension.clone().unwrap_or_default(),
        )),
        FileAttribute::Modified => Ok(FileValue::DateTime(file.modified)),
        // Compare permission bits only; the raw mode also carries file-type bits.
        FileAttribute::Permissions => Ok(FileValue::Number((file.permissions & 0o7777) as f64)),
        FileAttribute::IsDirectory => Ok(FileValue::Boolean(file.is_directory)),
        FileAttribute::Owner => {
            if let Some(owner) = &file.owner {
                Ok(FileValue::String(owner.clone()))
            } else {
                Ok(FileValue::Null)
            }
        }
        FileAttribute::IsExecutable => {
            // Check if file has executable bit set for user
            let is_executable = file.permissions & 0o100 != 0;
            Ok(FileValue::Boolean(is_executable))
        }
        _ => Err(ExecutorError::UnsupportedAttribute(format!(
            "Attribute not supported in conditions: {:?}",
            attribute
        ))),
    }
}

/// Compares two values.
fn compare_values(
    left: &FileValue,
    operator: &ComparisonOperator,
    right: &FileValue,
) -> Result<bool> {
    match (left, right) {
        (FileValue::String(l), FileValue::String(r)) => match operator {
            ComparisonOperator::Eq => Ok(l == r),
            ComparisonOperator::NotEq => Ok(l != r),
            ComparisonOperator::Lt => Ok(l < r),
            ComparisonOperator::LtEq => Ok(l <= r),
            ComparisonOperator::Gt => Ok(l > r),
            ComparisonOperator::GtEq => Ok(l >= r),
        },
        (FileValue::Number(l), FileValue::Number(r)) => match operator {
            ComparisonOperator::Eq => Ok(l == r),
            ComparisonOperator::NotEq => Ok(l != r),
            ComparisonOperator::Lt => Ok(l < r),
            ComparisonOperator::LtEq => Ok(l <= r),
            ComparisonOperator::Gt => Ok(l > r),
            ComparisonOperator::GtEq => Ok(l >= r),
        },
        (FileValue::DateTime(l), FileValue::DateTime(r)) => match operator {
            ComparisonOperator::Eq => Ok(l == r),
            ComparisonOperator::NotEq => Ok(l != r),
            ComparisonOperator::Lt => Ok(l < r),
            ComparisonOperator::LtEq => Ok(l <= r),
            ComparisonOperator::Gt => Ok(l > r),
            ComparisonOperator::GtEq => Ok(l >= r),
        },
        (FileValue::Boolean(l), FileValue::Boolean(r)) => match operator {
            ComparisonOperator::Eq => Ok(l == r),
            ComparisonOperator::NotEq => Ok(l != r),
            _ => Err(ExecutorError::UnsupportedOperation(format!(
                "Operator {:?} not supported for boolean values",
                operator
            ))),
        },
        (FileValue::Null, FileValue::Null) => match operator {
            ComparisonOperator::Eq => Ok(true),
            ComparisonOperator::NotEq => Ok(false),
            _ => Err(ExecutorError::UnsupportedOperation(
                "Null values only support equality comparisons".to_string(),
            )),
        },
        (_, FileValue::Null) | (FileValue::Null, _) => match operator {
            ComparisonOperator::Eq => Ok(false),
            ComparisonOperator::NotEq => Ok(true),
            _ => Err(ExecutorError::UnsupportedOperation(
                "Null values only support equality comparisons".to_string(),
            )),
        },
        // Coerce a String into DateTime when the other operand is DateTime.
        (FileValue::DateTime(l), FileValue::String(r)) => {
            if let Ok(parsed) = parse_date_string(r) {
                compare_values(
                    &FileValue::DateTime(*l),
                    operator,
                    &FileValue::DateTime(parsed),
                )
            } else {
                Err(ExecutorError::TypeError(format!(
                    "Cannot parse '{}' as a date for comparison with DateTime",
                    r
                )))
            }
        }
        (FileValue::String(l), FileValue::DateTime(r)) => {
            if let Ok(parsed) = parse_date_string(l) {
                compare_values(
                    &FileValue::DateTime(parsed),
                    operator,
                    &FileValue::DateTime(*r),
                )
            } else {
                Err(ExecutorError::TypeError(format!(
                    "Cannot parse '{}' as a date for comparison with DateTime",
                    l
                )))
            }
        }
        _ => Err(ExecutorError::TypeError(format!(
            "Cannot compare values of different types: {:?} and {:?}",
            left, right
        ))),
    }
}

/// Attempts to parse a date string into a `DateTime<Utc>`.
///
/// Supports the formats `YYYY-MM-DD` and full RFC 3339 / ISO 8601 timestamps.
fn parse_date_string(s: &str) -> std::result::Result<DateTime<Utc>, ExecutorError> {
    // Try full RFC 3339 first (e.g. "2025-01-01T00:00:00Z")
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return Ok(dt);
    }
    // Try bare date "YYYY-MM-DD" by appending midnight UTC
    if let Ok(dt) = format!("{}T00:00:00Z", s).parse::<DateTime<Utc>>() {
        return Ok(dt);
    }
    Err(ExecutorError::TypeError(format!(
        "Cannot parse '{}' as a date",
        s
    )))
}

// Include the tests module
#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "bug_regression_tests.rs"]
mod bug_regression_tests;
