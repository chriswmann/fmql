//! Executor for SQL-like file management commands.
//!
//! This module provides functionality to execute parsed SQL-like commands
//! on the file system, such as querying files or updating file attributes.

use chrono::{DateTime, Utc};
use regex::{Regex, RegexBuilder};
use serde::Serialize;
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

use crate::sql::ast::{
    ComparisonOperator, FileAttribute, FileAttributeUpdate, FileCondition, FileQuery, FileValue,
};

/// Errors that can occur during query execution.
#[derive(Error, Debug)]
pub enum ExecutorError {
    /// Error from `std::io` operations.
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

/// Executes a parsed `FileQuery`.
///
/// # Arguments
///
/// * `query` - The parsed `FileQuery` to execute.
///
/// # Returns
///
/// A Result containing a vector of `FileResult` instances or an `ExecutorError`.
///
/// # Errors
///
/// Returns an error if the directory cannot be read, a file's metadata cannot
/// be read, or an update requests an unsupported attribute or fails to apply.
///
/// # Examples
///
/// Example of using `execute_query` to process a parsed query:
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
    let predicate = condition.map(Predicate::compile).transpose()?;
    matching_files(path, recursive, predicate.as_ref())
}

/// Executes an UPDATE query.
///
/// Every update and the condition are validated, and every entry is evaluated,
/// before any file is changed.
fn execute_update(
    path: &Path,
    recursive: bool,
    updates: &[FileAttributeUpdate],
    condition: Option<&FileCondition>,
) -> Result<Vec<FileResult>> {
    let modes = updates
        .iter()
        .map(permission_mode)
        .collect::<Result<Vec<_>>>()?;
    let predicate = condition.map(Predicate::compile).transpose()?;
    let mut updated_files = Vec::new();
    if modes.is_empty() {
        return Ok(updated_files);
    }

    for file in matching_files(path, recursive, predicate.as_ref())? {
        // Setting permissions acts on a link's target, which may lie outside
        // the queried directory, so links are never updated.
        if fs::symlink_metadata(&file.path)?.file_type().is_symlink() {
            continue;
        }
        for &mode in &modes {
            fs::set_permissions(&file.path, Permissions::from_mode(mode))?;
        }
        // Re-read the file info to get updated attributes
        updated_files.push(create_file_result(&file.path)?);
    }

    Ok(updated_files)
}

/// Reads the octal mode an update sets, rejecting attributes that cannot be
/// written.
fn permission_mode(update: &FileAttributeUpdate) -> Result<u32> {
    match update.attribute {
        FileAttribute::Permissions => u32::from_str_radix(&update.value, 8)
            .ok()
            .filter(|mode| *mode <= 0o7777)
            .ok_or_else(|| {
                ExecutorError::TypeError(format!("Invalid permissions value: {}", update.value))
            }),
        FileAttribute::Owner => Err(ExecutorError::UnsupportedOperation(
            "Changing file ownership is not implemented".to_string(),
        )),
        _ => Err(ExecutorError::UnsupportedAttribute(format!(
            "Cannot update attribute: {:?}",
            update.attribute
        ))),
    }
}

/// Lists the entries under `path` that satisfy `predicate`.
///
/// Callers compile the predicate before calling, so an invalid condition fails
/// without touching the filesystem.
fn matching_files(
    path: &Path,
    recursive: bool,
    predicate: Option<&Predicate>,
) -> Result<Vec<FileResult>> {
    let files = list_files(path, recursive)?;
    Ok(match predicate {
        Some(predicate) => files
            .into_iter()
            .filter(|file| predicate.evaluate(file))
            .collect(),
        None => files,
    })
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
            ExecutorError::IoError(std::io::Error::other(format!(
                "Failed to read directory entry: {e}"
            )))
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

/// Creates a `FileResult` from a path.
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
        .map(std::string::ToString::to_string);

    let modified = metadata
        .modified()
        .map_or_else(|_| Utc::now(), DateTime::<Utc>::from);

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

/// The type of value an attribute produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Number,
    Time,
    Boolean,
}

/// Returns the kind of value `attribute` produces, or an error if its
/// metadata is not available.
fn attribute_kind(attribute: &FileAttribute) -> Result<Kind> {
    match attribute {
        FileAttribute::Name
        | FileAttribute::Path
        | FileAttribute::Extension
        | FileAttribute::Owner => Ok(Kind::Text),
        FileAttribute::Size | FileAttribute::Permissions => Ok(Kind::Number),
        FileAttribute::Modified => Ok(Kind::Time),
        FileAttribute::IsDirectory | FileAttribute::IsExecutable => Ok(Kind::Boolean),
        FileAttribute::All
        | FileAttribute::Created
        | FileAttribute::Accessed
        | FileAttribute::IsSymlink => Err(ExecutorError::UnsupportedAttribute(format!(
            "Attribute not supported in conditions: {attribute:?}"
        ))),
    }
}

/// A condition whose attributes, operand types and patterns have been
/// checked, so evaluating it against a file cannot fail.
#[derive(Debug)]
enum Predicate {
    Compare {
        attribute: FileAttribute,
        operator: ComparisonOperator,
        value: FileValue,
    },
    Between {
        attribute: FileAttribute,
        lower: FileValue,
        upper: FileValue,
    },
    Matches {
        attribute: FileAttribute,
        regex: Regex,
    },
    And(Box<Predicate>, Box<Predicate>),
    Or(Box<Predicate>, Box<Predicate>),
    Not(Box<Predicate>),
}

impl Predicate {
    /// Checks a condition and compiles its patterns. Every branch is checked,
    /// including ones that short-circuiting might never evaluate.
    fn compile(condition: &FileCondition) -> Result<Self> {
        match condition {
            FileCondition::Compare {
                attribute,
                operator,
                value,
            } => {
                let kind = attribute_kind(attribute)?;
                let value = operand(attribute, kind, value)?;
                let ordering =
                    !matches!(operator, ComparisonOperator::Eq | ComparisonOperator::NotEq);
                if ordering && (kind == Kind::Boolean || matches!(value, FileValue::Null)) {
                    return Err(ExecutorError::UnsupportedOperation(format!(
                        "Operator {operator} not supported for {attribute:?} = {value:?}"
                    )));
                }
                Ok(Self::Compare {
                    attribute: attribute.clone(),
                    operator: operator.clone(),
                    value,
                })
            }
            FileCondition::Between {
                attribute,
                lower,
                upper,
            } => {
                let kind = attribute_kind(attribute)?;
                let lower = operand(attribute, kind, lower)?;
                let upper = operand(attribute, kind, upper)?;
                if kind == Kind::Boolean
                    || matches!(lower, FileValue::Null)
                    || matches!(upper, FileValue::Null)
                {
                    return Err(ExecutorError::UnsupportedOperation(format!(
                        "BETWEEN not supported for {attribute:?}"
                    )));
                }
                Ok(Self::Between {
                    attribute: attribute.clone(),
                    lower,
                    upper,
                })
            }
            FileCondition::Like {
                attribute,
                pattern,
                case_sensitive,
            } => {
                text_attribute(attribute, "LIKE")?;
                Ok(Self::Matches {
                    attribute: attribute.clone(),
                    regex: like_regex(pattern, *case_sensitive)?,
                })
            }
            FileCondition::Regexp { attribute, pattern } => {
                text_attribute(attribute, "REGEXP")?;
                Ok(Self::Matches {
                    attribute: attribute.clone(),
                    regex: Regex::new(pattern)?,
                })
            }
            FileCondition::And(left, right) => Ok(Self::And(
                Box::new(Self::compile(left)?),
                Box::new(Self::compile(right)?),
            )),
            FileCondition::Or(left, right) => Ok(Self::Or(
                Box::new(Self::compile(left)?),
                Box::new(Self::compile(right)?),
            )),
            FileCondition::Not(inner) => Ok(Self::Not(Box::new(Self::compile(inner)?))),
        }
    }

    /// Evaluates the predicate against a file.
    fn evaluate(&self, file: &FileResult) -> bool {
        match self {
            Self::Compare {
                attribute,
                operator,
                value,
            } => compare_values(&attribute_value(file, attribute), operator, value),
            Self::Between {
                attribute,
                lower,
                upper,
            } => {
                let file_value = attribute_value(file, attribute);
                compare_values(&file_value, &ComparisonOperator::GtEq, lower)
                    && compare_values(&file_value, &ComparisonOperator::LtEq, upper)
            }
            Self::Matches { attribute, regex } => match attribute_value(file, attribute) {
                FileValue::String(s) => regex.is_match(&s),
                // An unknown owner matches no pattern.
                _ => false,
            },
            Self::And(left, right) => left.evaluate(file) && right.evaluate(file),
            Self::Or(left, right) => left.evaluate(file) || right.evaluate(file),
            Self::Not(inner) => !inner.evaluate(file),
        }
    }
}

/// Rejects pattern matching on attributes that are not strings.
fn text_attribute(attribute: &FileAttribute, operation: &str) -> Result<()> {
    if attribute_kind(attribute)? == Kind::Text {
        Ok(())
    } else {
        Err(ExecutorError::TypeError(format!(
            "{operation} can only be used with string attributes, got {attribute:?}"
        )))
    }
}

/// Checks that a literal suits an attribute of the given kind, parsing date
/// strings so they compare as times.
fn operand(attribute: &FileAttribute, kind: Kind, value: &FileValue) -> Result<FileValue> {
    match (kind, value) {
        (_, FileValue::Null)
        | (Kind::Text, FileValue::String(_))
        | (Kind::Number, FileValue::Number(_))
        | (Kind::Time, FileValue::DateTime(_))
        | (Kind::Boolean, FileValue::Boolean(_)) => Ok(value.clone()),
        (Kind::Time, FileValue::String(s)) => parse_date_string(s).map(FileValue::DateTime),
        _ => Err(ExecutorError::TypeError(format!(
            "Cannot compare {attribute:?} with {value:?}"
        ))),
    }
}

/// Converts a SQL LIKE pattern, where `%` matches any run of characters and
/// `_` matches one, into an anchored regular expression. Everything else is
/// literal.
fn like_regex(pattern: &str, case_sensitive: bool) -> Result<Regex> {
    let mut regex = String::from("^");
    for c in pattern.chars() {
        match c {
            '%' => regex.push_str(".*"),
            '_' => regex.push('.'),
            c => regex.push_str(&regex::escape(c.encode_utf8(&mut [0; 4]))),
        }
    }
    regex.push('$');
    // Wildcards may match a newline within a file name.
    Ok(RegexBuilder::new(&regex)
        .case_insensitive(!case_sensitive)
        .dot_matches_new_line(true)
        .build()?)
}

/// Gets the value of a file attribute. [`Predicate::compile`] has already
/// rejected attributes whose metadata is unavailable.
#[expect(
    clippy::cast_precision_loss,
    reason = "sizes above 2^53 bytes (8 PiB) are not a practical concern"
)]
fn attribute_value(file: &FileResult, attribute: &FileAttribute) -> FileValue {
    match attribute {
        FileAttribute::Name => FileValue::String(file.name.clone()),
        FileAttribute::Path => FileValue::String(file.path.to_string_lossy().to_string()),
        FileAttribute::Size => FileValue::Number(file.size as f64),
        FileAttribute::Extension => FileValue::String(file.extension.clone().unwrap_or_default()),
        FileAttribute::Modified => FileValue::DateTime(file.modified),
        // Compare permission bits only; the raw mode also carries file-type bits.
        FileAttribute::Permissions => FileValue::Number(f64::from(file.permissions & 0o7777)),
        FileAttribute::IsDirectory => FileValue::Boolean(file.is_directory),
        FileAttribute::Owner => file
            .owner
            .clone()
            .map_or(FileValue::Null, FileValue::String),
        // Check if file has executable bit set for user
        FileAttribute::IsExecutable => FileValue::Boolean(file.permissions & 0o100 != 0),
        FileAttribute::All
        | FileAttribute::Created
        | FileAttribute::Accessed
        | FileAttribute::IsSymlink => unreachable!("compile rejects {attribute:?}"),
    }
}

/// Applies an operator to two values of the same ordered type. Sizes and
/// permission bits are whole numbers, so exact float equality is intended.
fn ordered<T: PartialOrd + ?Sized>(left: &T, operator: &ComparisonOperator, right: &T) -> bool {
    match operator {
        ComparisonOperator::Eq => left == right,
        ComparisonOperator::NotEq => left != right,
        ComparisonOperator::Lt => left < right,
        ComparisonOperator::LtEq => left <= right,
        ComparisonOperator::Gt => left > right,
        ComparisonOperator::GtEq => left >= right,
    }
}

/// Compares a file's value with a literal of the kind [`operand`] accepted.
/// A null on either side equals only another null.
fn compare_values(left: &FileValue, operator: &ComparisonOperator, right: &FileValue) -> bool {
    match (left, right) {
        (FileValue::String(l), FileValue::String(r)) => ordered(l, operator, r),
        (FileValue::Number(l), FileValue::Number(r)) => ordered(l, operator, r),
        (FileValue::DateTime(l), FileValue::DateTime(r)) => ordered(l, operator, r),
        (FileValue::Boolean(l), FileValue::Boolean(r)) => ordered(l, operator, r),
        (FileValue::Null, FileValue::Null) => *operator == ComparisonOperator::Eq,
        (FileValue::Null, _) | (_, FileValue::Null) => *operator == ComparisonOperator::NotEq,
        _ => unreachable!("compile checks operand types: {left:?} {operator} {right:?}"),
    }
}

/// Attempts to parse a date string into a `DateTime<Utc>`.
///
/// Supports the formats `YYYY-MM-DD` and full RFC 3339 / ISO 8601 timestamps.
fn parse_date_string(s: &str) -> Result<DateTime<Utc>> {
    // Try full RFC 3339 first (e.g. "2025-01-01T00:00:00Z")
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return Ok(dt);
    }
    // Try bare date "YYYY-MM-DD" by appending midnight UTC
    if let Ok(dt) = format!("{s}T00:00:00Z").parse::<DateTime<Utc>>() {
        return Ok(dt);
    }
    Err(ExecutorError::TypeError(format!(
        "Cannot parse '{s}' as a date"
    )))
}

// Include the tests module
#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "bug_regression_tests.rs"]
mod bug_regression_tests;
