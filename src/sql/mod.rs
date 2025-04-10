//! SQL-like parsing and execution for file management operations.
//!
//! This module provides functionality to parse and execute SQL-like commands for managing
//! files and directories. It uses the sqlparser crate to parse SQL syntax and adapts it to
//! file system operations.

pub mod dialect;
pub mod parser;
pub mod executor;
pub mod ast;

// Re-exports for convenience
pub use parser::parse_sql;
pub use executor::execute_query; 