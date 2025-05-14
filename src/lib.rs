//! fmql - A fast and feature-rich file manager written in Rust.
//! 
//! This crate provides a command-line tool for managing files using a SQL-like query language.
//! 
//! # Examples
//! 
//! Using SQL-like queries:
//! ```no_run
//! use fmql::sql::{parse_sql, execute_query};
//! 
//! // Find all text files in the Documents folder
//! let query = parse_sql("SELECT * FROM ~/Documents WHERE extension = '.txt'").unwrap();
//! let results = execute_query(&query).unwrap();
//! 
//! for file in results {
//!     println!("{}", file.name);
//! }
//! ```

pub mod error;
pub mod sql; 