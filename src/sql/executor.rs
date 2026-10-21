//! Executor for SQL-like file management commands.
//!
//! This module provides functionality to execute parsed SQL-like commands
//! on the file system, such as querying files or updating file attributes.

use chrono::{DateTime, Utc};
use nix::unistd::{Uid, User};
use regex::{Regex, RegexBuilder};
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::{self, Permissions};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

use crate::sql::ast::{
    ComparisonOperator, FileAttribute, FileAttributeUpdate, FileCondition, FileQuery, FileValue,
    OrderBy,
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

    /// An update failed part way; every earlier change was undone.
    #[error("Failed to update {}: {source}; earlier changes were rolled back", path.display())]
    UpdateFailed {
        /// The file that could not be changed.
        path: PathBuf,
        /// Why the change failed.
        source: std::io::Error,
    },

    /// An update failed part way and some earlier changes could not be undone.
    #[error(
        "Failed to update {}: {source}; could not restore the previous permissions of {}",
        path.display(),
        display_paths(unrestored)
    )]
    RollbackFailed {
        /// The file that could not be changed.
        path: PathBuf,
        /// Why the change failed.
        source: std::io::Error,
        /// Files left with the new permissions.
        unrestored: Vec<PathBuf>,
    },
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
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
    /// The file creation time, if the filesystem records one.
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub created: Option<DateTime<Utc>>,
    /// The file access time, if available.
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub accessed: Option<DateTime<Utc>>,
    /// Whether the entry is a symbolic link. Other fields describe the link's
    /// target, or the link itself when the target is missing.
    pub is_symlink: bool,
    /// The name of the file's owner, if the user database knows its uid.
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
            order_by,
            limit,
        } => execute_select(
            path,
            *recursive,
            attributes,
            condition.as_ref(),
            order_by,
            *limit,
        ),
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
    order_by: &[OrderBy],
    limit: Option<usize>,
) -> Result<Vec<FileResult>> {
    for key in order_by {
        attribute_kind(&key.attribute)?;
    }
    let predicate = condition.map(Predicate::compile).transpose()?;
    let mut files = matching_files(path, recursive, predicate.as_ref())?;
    if !order_by.is_empty() {
        // A stable sort keeps traversal order between equal keys.
        files.sort_by(|a, b| {
            order_by
                .iter()
                .map(|key| {
                    let ordering = order_values(
                        &attribute_value(a, &key.attribute),
                        &attribute_value(b, &key.attribute),
                    );
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
        files.truncate(limit);
    }
    Ok(files)
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
    update_with(path, recursive, updates, condition, |path, mode| {
        fs::set_permissions(path, Permissions::from_mode(mode))
    })
}

/// [`execute_update`] with the permission setter supplied, so tests can make
/// a change fail part way through a real query.
fn update_with(
    path: &Path,
    recursive: bool,
    updates: &[FileAttributeUpdate],
    condition: Option<&FileCondition>,
    set_mode: impl FnMut(&Path, u32) -> std::io::Result<()>,
) -> Result<Vec<FileResult>> {
    let modes = updates
        .iter()
        .map(permission_mode)
        .collect::<Result<Vec<_>>>()?;
    let predicate = condition.map(Predicate::compile).transpose()?;
    // Later assignments win, so only the last mode needs applying.
    let Some(&mode) = modes.last() else {
        return Ok(Vec::new());
    };

    let mut targets: Vec<FileResult> = matching_files(path, recursive, predicate.as_ref())?
        .into_iter()
        // Setting permissions acts on a link's target, which may lie outside
        // the queried directory, so links are never updated.
        .filter(|file| !file.is_symlink)
        .collect();
    let paths: Vec<PathBuf> = targets.iter().map(|file| file.path.clone()).collect();
    apply_mode(&paths, mode, set_mode)?;

    // Only the permission bits changed. Updating the listed results, rather
    // than re-reading metadata, means a successful update cannot then fail.
    for file in &mut targets {
        file.permissions = (file.permissions & !0o7777) | mode;
    }
    Ok(targets)
}

/// Sets `mode` on every path, or on none of them: if a change fails, the paths
/// already changed get their previous permission bits back, newest first.
fn apply_mode(
    paths: &[PathBuf],
    mode: u32,
    mut set_mode: impl FnMut(&Path, u32) -> std::io::Result<()>,
) -> Result<()> {
    let mut changed: Vec<(&Path, u32)> = Vec::new();
    for path in paths {
        let previous = fs::metadata(path).map(|m| m.permissions().mode() & 0o7777);
        match previous.and_then(|previous| set_mode(path, mode).map(|()| previous)) {
            Ok(previous) => changed.push((path, previous)),
            Err(source) => {
                let unrestored: Vec<PathBuf> = changed
                    .iter()
                    .rev()
                    .filter(|(path, previous)| set_mode(path, *previous).is_err())
                    .map(|(path, _)| path.to_path_buf())
                    .collect();
                let path = path.clone();
                return Err(if unrestored.is_empty() {
                    ExecutorError::UpdateFailed { path, source }
                } else {
                    ExecutorError::RollbackFailed {
                        path,
                        source,
                        unrestored,
                    }
                });
            }
        }
    }
    Ok(())
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
    let mut owners = Owners::default();

    let walker = if recursive {
        WalkDir::new(dir_path)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
    } else {
        WalkDir::new(dir_path)
            .max_depth(1)
            .follow_links(false)
            .sort_by_file_name()
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

        let file_result = create_file_result(entry.path(), &mut owners)?;
        results.push(file_result);
    }

    Ok(results)
}

/// Looks up owner names, remembering each uid so a directory of files with
/// the same owner reads the user database once.
#[derive(Default)]
struct Owners(HashMap<u32, Option<String>>);

impl Owners {
    fn name(&mut self, uid: u32) -> Option<String> {
        self.0
            .entry(uid)
            .or_insert_with(|| {
                User::from_uid(Uid::from_raw(uid))
                    .ok()
                    .flatten()
                    .map(|user| user.name)
            })
            .clone()
    }
}

/// Creates a `FileResult` from a path.
fn create_file_result(path: &Path, owners: &mut Owners) -> Result<FileResult> {
    let link = fs::symlink_metadata(path)?;
    let is_symlink = link.file_type().is_symlink();
    // Describe a link by its target, falling back to the link itself when the
    // target is missing so one broken link cannot fail the whole query.
    let metadata = if is_symlink {
        fs::metadata(path).unwrap_or(link)
    } else {
        link
    };

    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());

    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().into_owned());

    let modified = metadata
        .modified()
        .map_or_else(|_| Utc::now(), DateTime::<Utc>::from);

    Ok(FileResult {
        path: path.to_path_buf(),
        name,
        size: metadata.len(),
        is_directory: metadata.is_dir(),
        extension,
        permissions: metadata.permissions().mode(),
        modified,
        created: timestamp(metadata.created()),
        accessed: timestamp(metadata.accessed()),
        is_symlink,
        owner: owners.name(metadata.uid()),
    })
}

/// Converts a timestamp the platform or filesystem may not provide.
fn timestamp(time: std::io::Result<std::time::SystemTime>) -> Option<DateTime<Utc>> {
    time.ok().map(DateTime::<Utc>::from)
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
        FileAttribute::Modified | FileAttribute::Created | FileAttribute::Accessed => {
            Ok(Kind::Time)
        }
        FileAttribute::IsDirectory | FileAttribute::IsExecutable | FileAttribute::IsSymlink => {
            Ok(Kind::Boolean)
        }
        FileAttribute::All => Err(ExecutorError::UnsupportedAttribute(
            "* is not an attribute that can be compared".to_string(),
        )),
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

/// Gets the value of a file attribute. Metadata the platform does not
/// provide is null. [`Predicate::compile`] has already rejected `*`.
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
        FileAttribute::Created => file.created.map_or(FileValue::Null, FileValue::DateTime),
        FileAttribute::Accessed => file.accessed.map_or(FileValue::Null, FileValue::DateTime),
        FileAttribute::IsSymlink => FileValue::Boolean(file.is_symlink),
        FileAttribute::All => unreachable!("compile rejects {attribute:?}"),
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

/// Orders two values of one attribute, placing nulls after everything else.
fn order_values(left: &FileValue, right: &FileValue) -> Ordering {
    match (left, right) {
        (FileValue::String(l), FileValue::String(r)) => l.cmp(r),
        (FileValue::Number(l), FileValue::Number(r)) => l.total_cmp(r),
        (FileValue::DateTime(l), FileValue::DateTime(r)) => l.cmp(r),
        (FileValue::Boolean(l), FileValue::Boolean(r)) => l.cmp(r),
        (FileValue::Null, FileValue::Null) => Ordering::Equal,
        (FileValue::Null, _) => Ordering::Greater,
        (_, FileValue::Null) => Ordering::Less,
        _ => unreachable!("one attribute yields one kind: {left:?} {right:?}"),
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
