//! ls-rs - A fast and feature-rich file manager written in Rust.
//! 
//! This crate provides a command-line tool for listing and managing files with various
//! features like sorting, grouping, and detailed file information. It also includes a
//! SQL-like query language for more powerful file management operations.
//! 
//! # Examples
//! 
//! Basic file listing:
//! ```rust
//! use ls_rs::cli::{Args, GroupByOption, SortOption, OutputFormat};
//! use ls_rs::file::list_directory;
//! use std::path::PathBuf;
//! 
//! let args = Args {
//!     path: PathBuf::from("."),
//!     long_view: false,
//!     show_total: false,
//!     recursive: false,
//!     show_hidden: false,
//!     group_by: GroupByOption::None,
//!     name_pattern: None,
//!     sort_by: SortOption::Name,
//!     output_format: OutputFormat::Text,
//! };
//! 
//! let files = list_directory(&args).unwrap();
//! 
//! for file in files {
//!     println!("{}", file.name);
//! }
//! ```
//!
//! Using SQL-like queries:
//! ```no_run
//! use ls_rs::sql::{parse_sql, execute_query};
//! 
//! // Find all text files in the Documents folder
//! let query = parse_sql("SELECT * FROM ~/Documents WHERE extension = '.txt'").unwrap();
//! let results = execute_query(&query).unwrap();
//! 
//! for file in results {
//!     println!("{}", file.name);
//! }
//! ```

pub mod cli;
pub mod display;
pub mod error;
pub mod file;
pub mod sql; 