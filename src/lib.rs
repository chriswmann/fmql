//! fmql - A fast and feature-rich file manager written in Rust.
//!
//! This crate provides a command-line tool for managing files using a SQL-like query language.
//!
//! # Examples
//!
//! Using SQL-like queries:
//! ```no_run
//! // Find all text files in the Documents folder
//! let results = fmql::query("SELECT * FROM ~/Documents WHERE extension = 'txt'").unwrap();
//!
//! for file in results {
//!     println!("{}", file.name);
//! }
//! ```

pub mod error;
pub mod output;
pub mod sql;

pub use error::{FMQLError, Result};

use sql::executor::FileResult;

/// Parses and executes one query.
///
/// A `SELECT` returns the matching entries. An `UPDATE` returns the entries it
/// changed, as they are after the change.
///
/// # Errors
///
/// Returns [`FMQLError::Parse`] if the query is malformed or unsupported, and
/// [`FMQLError::Execute`] if it cannot be carried out.
pub fn query(sql: &str) -> Result<Vec<FileResult>> {
    let query = sql::parse_sql(sql)?;
    Ok(sql::execute_query(&query)?)
}
