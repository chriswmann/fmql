//! SQL-like parsing and execution for file management operations.
//!
//! This module provides functionality to parse and execute SQL-like commands for managing
//! files and directories. It uses the sqlparser crate to parse SQL syntax and adapts it to
//! file system operations.
//!
//! # Components
//! 
//! - `dialect`: Defines a custom SQL dialect for file operations
//! - `parser`: Parses SQL strings into abstract syntax tree (AST) structures
//! - `executor`: Executes the parsed queries against the file system
//! - `ast`: Defines the abstract syntax tree data structures
//!
//! # Examples
//!
//! Querying files using SQL-like syntax:
//!
//! ```no_run
//! use fmql::sql::{parse_sql, execute_query};
//!
//! // Find all text files in the current directory
//! let query = parse_sql("SELECT * FROM . WHERE name LIKE '%.txt'").unwrap();
//! let results = execute_query(&query).unwrap();
//!
//! // Display results
//! for file in results {
//!     println!("{}: {} bytes", file.name, file.size);
//! }
//! ```
//!
//! Using a more complex query with conditions:
//!
//! ```no_run
//! use fmql::sql::{parse_sql, execute_query};
//!
//! // Find large image files, recursively
//! let sql = "WITH RECURSIVE SELECT * FROM ~/Pictures WHERE \
//!            (extension = 'jpg' OR extension = 'png') AND size > 1000000";
//!            
//! let query = parse_sql(sql).unwrap();
//! let results = execute_query(&query).unwrap();
//!
//! println!("Found {} large image files", results.len());
//! ```
//!
//! Updating file permissions:
//!
//! ```no_run
//! use fmql::sql::{parse_sql, execute_query};
//!
//! // Make all shell scripts executable
//! let sql = "UPDATE . SET permissions = '755' WHERE extension = 'sh'";
//! let query = parse_sql(sql).unwrap();
//! let updated_files = execute_query(&query).unwrap();
//!
//! println!("Updated permissions for {} files", updated_files.len());
//! ```

pub mod dialect;
pub mod parser;
pub mod executor;
pub mod ast;

// Re-exports for convenience
pub use parser::parse_sql;
pub use executor::execute_query; 