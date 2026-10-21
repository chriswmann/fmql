//! Abstract Syntax Tree (AST) structures for file management operations.
//!
//! This module defines the custom AST structures that represent parsed SQL-like
//! file management operations, such as querying files or updating file attributes.
//! These structures form the intermediary representation between the SQL parser
//! and the executor that performs the actual file operations.
//!
//! # Structure
//!
//! The AST is organized hierarchically:
//! - `FileQuery` is the top-level structure representing complete queries
//! - `FileCondition` represents filtering conditions (WHERE clauses)
//! - `FileAttribute` represents file properties that can be queried or updated
//! - `FileValue` represents literal values used in conditions
//!
//! # Examples
//!
//! Building a query programmatically:
//!
//! ```no_run
//! use fmql::sql::ast::{FileQuery, FileAttribute, FileCondition, ComparisonOperator, FileValue};
//! use std::path::PathBuf;
//!
//! // Equivalent to: SELECT * FROM ~/Documents WHERE size > 1000000
//! let query = FileQuery::Select {
//!     path: PathBuf::from("~/Documents"),
//!     recursive: false,
//!     attributes: vec![FileAttribute::All],
//!     condition: Some(FileCondition::Compare {
//!         attribute: FileAttribute::Size,
//!         operator: ComparisonOperator::Gt,
//!         value: FileValue::Number(1000000.0),
//!     }),
//!     order_by: Vec::new(),
//!     limit: None,
//! };
//!
//! // Now use the query with an executor...
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The main query structure representing a complete file management operation.
///
/// This enum represents the two main types of queries supported:
/// - `Select`: For retrieving files matching certain criteria
/// - `Update`: For modifying files matching certain criteria
///
/// Each query type contains information about the target path, conditions,
/// and either attributes to retrieve or updates to apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileQuery {
    /// A query to select files matching specific criteria.
    ///
    /// # Examples
    ///
    /// This represents a query like:
    /// ```sql
    /// SELECT name, size FROM ~/Documents WHERE extension = 'pdf'
    /// ```
    ///
    /// Or with recursion:
    /// ```sql
    /// WITH RECURSIVE SELECT * FROM . WHERE size > 1000000
    /// ```
    Select {
        /// The directory path to search in.
        path: PathBuf,
        /// Whether to search recursively through subdirectories.
        recursive: bool,
        /// The file attributes to return (empty means all).
        attributes: Vec<FileAttribute>,
        /// The conditions to filter files by (None means all files).
        condition: Option<FileCondition>,
        /// Sort keys, most significant first. Without any, entries are in
        /// traversal order, which visits each directory's entries by name.
        #[serde(default)]
        order_by: Vec<OrderBy>,
        /// The maximum number of entries to return, applied after sorting.
        #[serde(default)]
        limit: Option<usize>,
    },

    /// A query to update file attributes for files matching specific criteria.
    ///
    /// # Examples
    ///
    /// This represents a query like:
    /// ```sql
    /// UPDATE ~/scripts SET permissions = '755' WHERE extension = 'sh'
    /// ```
    Update {
        /// The directory path containing files to update.
        path: PathBuf,
        /// Whether to search recursively through subdirectories.
        recursive: bool,
        /// The attributes to update and their new values.
        updates: Vec<FileAttributeUpdate>,
        /// The conditions to filter files by (None means all files).
        condition: Option<FileCondition>,
    },
}

/// Represents a file attribute that can be queried or displayed.
///
/// These attributes correspond to file metadata and properties that can be
/// included in query results or used in filtering conditions.
///
/// # Examples
///
/// ```no_run
/// use fmql::sql::ast::FileAttribute;
///
/// // In a SELECT query:
/// // SELECT name, size, modified FROM ...
/// let attributes = vec![
///     FileAttribute::Name,
///     FileAttribute::Size,
///     FileAttribute::Modified
/// ];
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FileAttribute {
    /// All attributes (*).
    All,
    /// The file name without path.
    Name,
    /// The full file path.
    Path,
    /// The file size in bytes.
    Size,
    /// The file extension (part after the last dot).
    Extension,
    /// The file modification time.
    Modified,
    /// The file creation time.
    Created,
    /// The file access time.
    Accessed,
    /// The file permissions in octal format.
    Permissions,
    /// The type and permissions as `ls -l` shows them, such as `drwxr-x---`.
    Mode,
    /// The file owner username.
    Owner,
    /// Whether the file is a directory.
    IsDirectory,
    /// Whether the file is a symbolic link.
    IsSymlink,
    /// Whether the file is executable.
    IsExecutable,
}

impl FileAttribute {
    /// The attribute's name in queries and JSON output; `*` for [`Self::All`].
    ///
    /// ```
    /// use fmql::sql::ast::FileAttribute;
    ///
    /// assert_eq!(FileAttribute::IsSymlink.name(), "is_symlink");
    /// ```
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::All => "*",
            Self::Name => "name",
            Self::Path => "path",
            Self::Size => "size",
            Self::Extension => "extension",
            Self::Modified => "modified",
            Self::Created => "created",
            Self::Accessed => "accessed",
            Self::Permissions => "permissions",
            Self::Mode => "mode",
            Self::Owner => "owner",
            Self::IsDirectory => "is_directory",
            Self::IsSymlink => "is_symlink",
            Self::IsExecutable => "is_executable",
        }
    }
}

/// A sort key in an `ORDER BY` clause.
///
/// Nulls sort after every other value in ascending order and before them in
/// descending order, as in PostgreSQL.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderBy {
    /// The attribute to sort by.
    pub attribute: FileAttribute,
    /// Whether larger values come first.
    pub descending: bool,
}

/// Represents an update operation for a file attribute.
///
/// This structure defines which attribute to update and what value
/// to set it to.
///
/// # Examples
///
/// ```no_run
/// use fmql::sql::ast::{FileAttributeUpdate, FileAttribute, UpdateValue};
///
/// // In an UPDATE query:
/// // UPDATE ... SET permissions = '755'
/// let update = FileAttributeUpdate {
///     attribute: FileAttribute::Permissions,
///     value: UpdateValue::Literal("755".to_string()),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAttributeUpdate {
    /// The attribute to update.
    pub attribute: FileAttribute,
    /// The new value for the attribute.
    pub value: UpdateValue,
}

/// The value an update assigns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UpdateValue {
    /// A fixed value: octal digits for `permissions`, a user name for `owner`.
    Literal(String),
    /// A new file name computed from each file, for `name`.
    Name(NameExpr),
}

/// An expression computing a file's new name in `SET name = ...`.
///
/// ```
/// use fmql::sql::ast::{FileAttribute, NameExpr};
///
/// // REPLACE(name, '.jpeg', '.jpg')
/// let rename = NameExpr::Replace {
///     expr: Box::new(NameExpr::Attribute(FileAttribute::Name)),
///     from: ".jpeg".to_string(),
///     to: ".jpg".to_string(),
/// };
/// # let _ = rename;
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NameExpr {
    /// A string literal.
    Literal(String),
    /// The file's `name` or `extension` (empty when it has none).
    Attribute(FileAttribute),
    /// The parts joined in order, from `a || b` or `CONCAT(a, b, ...)`.
    Concat(Vec<NameExpr>),
    /// `REPLACE(expr, from, to)`: every occurrence of `from` replaced by `to`.
    Replace {
        /// The string to search.
        expr: Box<NameExpr>,
        /// The text to find; never empty.
        from: String,
        /// Its replacement.
        to: String,
    },
    /// `LOWER(expr)`.
    Lower(Box<NameExpr>),
    /// `UPPER(expr)`.
    Upper(Box<NameExpr>),
}

/// Represents a condition for filtering files.
///
/// This enum represents the various types of conditions that can appear
/// in the WHERE clause of a query, including simple comparisons, logical
/// operations, and pattern matching.
///
/// # Examples
///
/// ```no_run
/// use fmql::sql::ast::{FileCondition, FileAttribute, ComparisonOperator, FileValue};
///
/// // WHERE size > 1000000
/// let condition = FileCondition::Compare {
///     attribute: FileAttribute::Size,
///     operator: ComparisonOperator::Gt,
///     value: FileValue::Number(1000000.0),
/// };
///
/// // WHERE name LIKE '%.txt'
/// let text_files = FileCondition::Like {
///     attribute: FileAttribute::Name,
///     pattern: "%.txt".to_string(),
///     case_sensitive: false,
/// };
///
/// // Complex condition: WHERE (size > 1000000 AND extension = 'pdf')
/// let complex = FileCondition::And(
///     Box::new(condition),
///     Box::new(FileCondition::Compare {
///         attribute: FileAttribute::Extension,
///         operator: ComparisonOperator::Eq,
///         value: FileValue::String("pdf".to_string()),
///     })
/// );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
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
///
/// These operators define how attributes are compared to values
/// in query conditions.
///
/// # Examples
///
/// ```no_run
/// use fmql::sql::ast::ComparisonOperator;
///
/// // Using operators in conditions:
/// assert_eq!(ComparisonOperator::Eq.to_string(), "=");
/// assert_eq!(ComparisonOperator::Gt.to_string(), ">");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComparisonOperator {
    /// Equal to (=).
    Eq,
    /// Not equal to (!=).
    NotEq,
    /// Less than (<).
    Lt,
    /// Less than or equal to (<=).
    LtEq,
    /// Greater than (>).
    Gt,
    /// Greater than or equal to (>=).
    GtEq,
}

impl std::fmt::Display for ComparisonOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComparisonOperator::Eq => write!(f, "="),
            ComparisonOperator::NotEq => write!(f, "!="),
            ComparisonOperator::Lt => write!(f, "<"),
            ComparisonOperator::LtEq => write!(f, "<="),
            ComparisonOperator::Gt => write!(f, ">"),
            ComparisonOperator::GtEq => write!(f, ">="),
        }
    }
}

/// Represents a value in a file condition.
///
/// This enum represents the different types of literal values that
/// can be used in file conditions, such as strings, numbers, and dates.
///
/// # Examples
///
/// ```no_run
/// use fmql::sql::ast::FileValue;
/// use chrono::Utc;
///
/// // Different value types:
/// let string_value = FileValue::String("example.txt".to_string());
/// let number_value = FileValue::Number(1024.0);
/// let date_value = FileValue::DateTime(Utc::now());
/// let bool_value = FileValue::Boolean(true);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
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
