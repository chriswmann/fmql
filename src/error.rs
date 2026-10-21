//! The crate-wide error type.
//!
//! Parsing and execution keep their own error enums, [`ParserError`] and
//! [`ExecutorError`]. [`FMQLError`] wraps both so callers that run a query
//! from start to finish, such as [`crate::query`] and the CLI, handle one type.

use thiserror::Error;

use crate::sql::executor::ExecutorError;
use crate::sql::parser::ParserError;

/// An error from parsing or executing a query.
///
/// # Examples
///
/// ```
/// use fmql::error::FMQLError;
///
/// let err = fmql::query("DELETE FROM .").unwrap_err();
/// assert!(matches!(err, FMQLError::Parse(_)));
/// assert!(err.to_string().starts_with("Error parsing SQL query"));
/// ```
#[derive(Error, Debug)]
pub enum FMQLError {
    /// The query was malformed or uses unsupported syntax.
    #[error("Error parsing SQL query: {0}")]
    Parse(#[from] ParserError),

    /// The query was valid but could not be carried out.
    #[error("Error executing query: {0}")]
    Execute(#[from] ExecutorError),
}

/// Result type for operations that parse and execute a query.
pub type Result<T> = std::result::Result<T, FMQLError>;
