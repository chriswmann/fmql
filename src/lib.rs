//! ls-rs - A fast and feature-rich file manager written in Rust.
//! 
//! This crate provides a command-line tool for listing and managing files with various
//! features like sorting, grouping, and detailed file information.
//! 
//! # Examples
//! 
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

pub mod file;
pub mod display;
pub mod app;
pub mod cli;
pub mod error;

pub use file::FileInfo;
pub use display::format_size;
pub use cli::Args;
pub use error::{FileManagerError, Result}; 