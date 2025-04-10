//! Abstract Syntax Tree (AST) structures for file management operations.
//!
//! This module defines the custom AST structures that represent parsed SQL-like
//! file management operations, such as querying files or updating file attributes.

use std::path::PathBuf;
use chrono::{DateTime, Utc};

/// The main query structure representing a complete file management operation.
#[derive(Debug, Clone)]
pub enum FileQuery {
    /// A query to select files matching specific criteria.
    Select {
        /// The directory path to search in.
        path: PathBuf,
        /// Whether to search recursively.
        recursive: bool,
        /// The file attributes to return.
        attributes: Vec<FileAttribute>,
        /// The conditions to filter files by.
        condition: Option<FileCondition>,
    },
    /// A query to update file attributes for files matching specific criteria.
    Update {
        /// The directory path containing files to update.
        path: PathBuf,
        /// The attributes to update and their new values.
        updates: Vec<FileAttributeUpdate>,
        /// The conditions to filter files by.
        condition: Option<FileCondition>,
    },
}

/// Represents a file attribute that can be queried or displayed.
#[derive(Debug, Clone, PartialEq)]
pub enum FileAttribute {
    /// All attributes (*).
    All,
    /// The file name.
    Name,
    /// The file path.
    Path,
    /// The file size in bytes.
    Size,
    /// The file extension.
    Extension,
    /// The file modification time.
    Modified,
    /// The file creation time.
    Created,
    /// The file access time.
    Accessed,
    /// The file permissions.
    Permissions,
    /// The file owner.
    Owner,
    /// Whether the file is a directory.
    IsDirectory,
    /// Whether the file is a symlink.
    IsSymlink,
    /// Whether the file is executable.
    IsExecutable,
}

/// Represents an update operation for a file attribute.
#[derive(Debug, Clone)]
pub struct FileAttributeUpdate {
    /// The attribute to update.
    pub attribute: FileAttribute,
    /// The new value for the attribute.
    pub value: String,
}

/// Represents a condition for filtering files.
#[derive(Debug, Clone)]
pub enum FileCondition {
    /// Compares a file attribute to a value.
    Compare {
        /// The attribute to compare.
        attribute: FileAttribute,
        /// The comparison operator.
        operator: ComparisonOperator,
        /// The value to compare against.
        value: FileValue,
    },
    /// A logical AND of multiple conditions.
    And(Box<FileCondition>, Box<FileCondition>),
    /// A logical OR of multiple conditions.
    Or(Box<FileCondition>, Box<FileCondition>),
    /// A logical NOT of a condition.
    Not(Box<FileCondition>),
    /// A LIKE pattern matching condition.
    Like {
        /// The attribute to match.
        attribute: FileAttribute,
        /// The pattern to match against.
        pattern: String,
        /// Whether the match is case-sensitive.
        case_sensitive: bool,
    },
    /// A BETWEEN condition (value is between two bounds).
    Between {
        /// The attribute to check.
        attribute: FileAttribute,
        /// The lower bound.
        lower: FileValue,
        /// The upper bound.
        upper: FileValue,
    },
    /// A regular expression matching condition.
    Regexp {
        /// The attribute to match.
        attribute: FileAttribute,
        /// The regular expression to match against.
        pattern: String,
    },
}

/// Comparison operators for file conditions.
#[derive(Debug, Clone, PartialEq)]
pub enum ComparisonOperator {
    /// Equal to.
    Eq,
    /// Not equal to.
    NotEq,
    /// Less than.
    Lt,
    /// Less than or equal to.
    LtEq,
    /// Greater than.
    Gt,
    /// Greater than or equal to.
    GtEq,
}

/// Represents a value in a file condition.
#[derive(Debug, Clone)]
pub enum FileValue {
    /// A string value.
    String(String),
    /// A numeric value.
    Number(f64),
    /// A date/time value.
    DateTime(DateTime<Utc>),
    /// A boolean value.
    Boolean(bool),
    /// A null value.
    Null,
} 