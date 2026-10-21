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

use sql::ast::FileQuery;
use sql::executor::{FileResult, Table};

/// Parses and executes one query that returns files.
///
/// A `SELECT` returns the matching entries. An `UPDATE` returns the entries it
/// changed, as they are after the change. Use [`run`] for queries that may
/// group.
///
/// # Errors
///
/// Returns [`FMQLError::Parse`] if the query is malformed or unsupported, and
/// [`FMQLError::Execute`] if it cannot be carried out, including when it is a
/// grouped query.
pub fn query(sql: &str) -> Result<Vec<FileResult>> {
    let query = sql::parse_sql(sql)?;
    Ok(sql::execute_query(&query)?)
}

/// What a query returns: files, or rows for a grouped query.
#[derive(Debug, Clone)]
pub enum Output {
    /// Entries from a `SELECT` or `UPDATE`.
    Files(Vec<FileResult>),
    /// Rows from a query with `GROUP BY` or aggregates.
    Table(Table),
}

/// Parses and executes any query.
///
/// ```no_run
/// let fmql::Output::Table(table) =
///     fmql::run("SELECT extension, SUM(size) FROM . GROUP BY extension").unwrap()
/// else {
///     unreachable!()
/// };
/// println!("{} extensions", table.rows.len());
/// ```
///
/// # Errors
///
/// Returns [`FMQLError::Parse`] if the query is malformed or unsupported, and
/// [`FMQLError::Execute`] if it cannot be carried out.
pub fn run(sql: &str) -> Result<Output> {
    let query = sql::parse_sql(sql)?;
    Ok(match query {
        FileQuery::Group { .. } => Output::Table(sql::execute_grouped(&query)?),
        _ => Output::Files(sql::execute_query(&query)?),
    })
}
