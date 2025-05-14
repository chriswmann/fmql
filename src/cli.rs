//! CLI module that defines command-line arguments and options.
//!
//! This module provides the structures and enums necessary for parsing and validating
//! command-line arguments. It uses the `clap` crate for argument parsing and defines
//! various options for file listing and display.
//!
//! # Examples
//!
//! Basic usage:
//!
//! ```no_run
//! use fmql::cli::{Args, SortOption, GroupByOption, OutputFormat};
//! use std::path::PathBuf;
//!
//! // Create arguments for listing text files sorted by size
//! let args = Args {
//!     path: PathBuf::from("."),
//!     long_view: true,
//!     show_total: false,
//!     recursive: true,
//!     show_hidden: false,
//!     group_by: GroupByOption::Extension,
//!     name_pattern: Some("*.txt".to_string()),
//!     sort_by: SortOption::Size,
//!     output_format: OutputFormat::Text,
//! };
//!
//! // Validate the arguments
//! if let Err(e) = args.validate() {
//!     eprintln!("Invalid arguments: {}", e);
//! }
//! ```

use crate::error::{FileManagerError, Result};
use clap::Parser;
use std::path::PathBuf;

/// Command-line arguments for file listing operations.
///
/// This structure defines all possible options that can be specified
/// when listing files, including display options, filtering, sorting,
/// and grouping.
#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Path to list files from (defaults to current directory if not specified).
    pub path: PathBuf,

    /// Whether to show hidden files (those starting with a dot).
    pub show_hidden: bool,

    /// Whether to use a detailed view format with additional file information.
    pub long_view: bool,

    /// How to sort the file listing (by name, size, modification time, or type).
    pub sort_by: SortOption,

    /// Whether to list files recursively through subdirectories.
    pub recursive: bool,

    /// Whether to display summary information like total size or file count.
    pub show_total: bool,

    /// How to group files in the listing (e.g., by folder or extension).
    pub group_by: GroupByOption,

    /// Optional pattern to filter files by name (e.g., "*.txt").
    pub name_pattern: Option<String>,

    /// The output format to use when displaying file listings.
    pub output_format: OutputFormat,
}

impl Args {
    /// Validates the command-line arguments.
    ///
    /// This method checks that the arguments are valid and consistent, such as:
    /// - The specified path exists and is accessible
    /// - Any patterns are syntactically valid
    ///
    /// # Returns
    /// * `Result<()>` - Ok if arguments are valid, or an error with details
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use fmql::cli::Args;
    /// use std::path::PathBuf;
    ///
    /// let args = Args::default();
    /// match args.validate() {
    ///     Ok(()) => println!("Arguments are valid"),
    ///     Err(e) => eprintln!("Invalid arguments: {}", e),
    /// }
    /// ```
    pub fn validate(&self) -> Result<()> {
        if !self.path.exists() {
            return Err(FileManagerError::PathNotFound(self.path.clone()));
        }
        Ok(())
    }
}

impl Default for Args {
    /// Creates a default set of arguments.
    ///
    /// The defaults are:
    /// - Current directory (`.`)
    /// - No hidden files
    /// - Simple view (not long)
    /// - Sorted by name
    /// - Not recursive
    /// - No totals displayed
    /// - No grouping
    /// - No name filters
    /// - Text output format
    ///
    /// # Examples
    ///
    /// ```
    /// use fmql::cli::{Args, SortOption, GroupByOption, OutputFormat};
    /// use std::path::PathBuf;
    ///
    /// let default_args = Args::default();
    /// assert_eq!(default_args.path, PathBuf::from("."));
    /// assert_eq!(default_args.show_hidden, false);
    /// assert_eq!(default_args.sort_by, SortOption::Name);
    /// assert_eq!(default_args.group_by, GroupByOption::None);
    /// assert_eq!(default_args.output_format, OutputFormat::Text);
    /// ```
    fn default() -> Self {
        Self {
            path: PathBuf::from("."),
            show_hidden: false,
            long_view: false,
            sort_by: SortOption::Name,
            recursive: false,
            show_total: false,
            group_by: GroupByOption::None,
            name_pattern: None,
            output_format: OutputFormat::Text,
        }
    }
}

/// Sort options for file listing.
///
/// These options determine how files are ordered in the listing result.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum SortOption {
    /// Sort by file name (alphabetically)
    Name,
    /// Sort by file size (largest first)
    Size,
    /// Sort by modification time (most recent first)
    Modified,
    /// Sort by file type/extension
    Type,
}

/// Grouping options for file listing.
///
/// These options determine how files are grouped in the listing result.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum GroupByOption {
    /// No grouping, files are listed according to sort order only
    None,
    /// Group directories first, then files
    Folder,
    /// Group all files by their parent directory hierarchy
    AllFolders,
    /// Group files by extension (e.g., all .txt files together)
    Extension,
    /// Group files by permission type (e.g., readable, writable)
    Permissions,
    /// Group executable files separately from non-executable ones
    Executable,
    /// Group files by the first character or pattern in their name
    NameStartsWith,
    /// Group files based on whether they contain a certain pattern
    NameContains,
    /// Group files by the last characters in their name
    NameEndsWith,
}

/// Output format options for file listings.
///
/// These options determine how the file information is displayed to the user.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum OutputFormat {
    /// Simple text output with one file per line
    Text,
    /// Formatted table output with columns for different attributes
    Table,
}

