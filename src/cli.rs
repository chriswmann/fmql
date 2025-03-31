//! Command-line interface for the file manager.
//! 
//! This module provides the command-line argument parsing and validation functionality.
//! 
//! # Examples
//! 
//! ```rust
//! use ls_rs::cli::{Args, GroupByOption, SortOption, OutputFormat};
//! use std::path::PathBuf;
//! 
//! let args = Args {
//!     path: PathBuf::from("."),
//!     group_by: GroupByOption::Extension,
//!     sort_by: SortOption::Size,
//!     output_format: OutputFormat::Text,
//!     long_view: false,
//!     show_total: false,
//!     recursive: false,
//!     show_hidden: false,
//!     name_pattern: None,
//! };
//! ```

use clap::Parser;
use std::path::PathBuf;
use crate::error::{FileManagerError, Result};

/// Command-line arguments for the file manager.
/// 
/// This struct represents all possible command-line arguments that can be passed to the file manager.
/// 
/// # Examples
/// 
/// ```rust
/// use ls_rs::cli::{Args, GroupByOption, SortOption, OutputFormat};
/// use std::path::PathBuf;
/// 
/// let args = Args {
///     path: PathBuf::from("."),
///     long_view: false,
///     show_total: false,
///     recursive: false,
///     show_hidden: false,
///     group_by: GroupByOption::None,
///     name_pattern: None,
///     sort_by: SortOption::Name,
///     output_format: OutputFormat::Text,
/// };
/// ```
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Path to list (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Show hidden files (files starting with '.')
    #[arg(short = 'a', long = "all")]
    pub show_hidden: bool,

    /// Use detailed view (shows more information)
    #[arg(short = 'l', long = "long")]
    pub long_view: bool,

    /// Sort by: name, size, modified, type (default: name)
    #[arg(short = 's', long = "sort", default_value = "name")]
    pub sort_by: SortOption,

    /// Recursively list directories
    #[arg(short = 'r', long = "recursive")]
    pub recursive: bool,

    /// Show total size of all files
    #[arg(short = 't', long = "total")]
    pub show_total: bool,

    /// Group totals by: none, folder, all-folders, extension, permissions, executable, name-starts-with, name-contains, name-ends-with (default: none)
    #[arg(short = 'g', long = "group-by", default_value = "none")]
    pub group_by: GroupByOption,

    /// Pattern for name-based grouping (required for name-starts-with, name-contains, name-ends-with)
    #[arg(long = "pattern", required_if_eq("group_by", "name-starts-with"), 
          required_if_eq("group_by", "name-contains"),
          required_if_eq("group_by", "name-ends-with"))]
    pub name_pattern: Option<String>,

    /// Output format: text, table (default: text)
    #[arg(short = 'f', long = "format", default_value = "text")]
    pub output_format: OutputFormat,
}

impl Args {
    /// Validates the command line arguments.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use ls_rs::cli::{Args, GroupByOption, SortOption, OutputFormat};
    /// use std::path::PathBuf;
    /// 
    /// let mut args = Args {
    ///     path: PathBuf::from("."),
    ///     long_view: false,
    ///     show_total: false,
    ///     recursive: false,
    ///     show_hidden: false,
    ///     group_by: GroupByOption::None,
    ///     name_pattern: None,
    ///     sort_by: SortOption::Name,
    ///     output_format: OutputFormat::Text,
    /// };
    /// 
    /// assert!(args.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<()> {
        if let Some(pattern) = &self.name_pattern {
            if pattern.is_empty() {
                return Err(FileManagerError::InvalidPattern("Pattern cannot be empty".to_string()));
            }
        }

        if !self.path.exists() {
            return Err(FileManagerError::PathNotFound(self.path.clone()));
        }

        Ok(())
    }

    /// Create a new Args instance for testing.
    /// 
    /// This method is only available when running tests.
    /// 
    /// # Arguments
    /// 
    /// * `path` - The path to use for the Args instance.
    /// 
    /// # Returns
    /// 
    /// A new Args instance with default values and the specified path.
    #[cfg(test)]
    pub fn new_for_test(path: PathBuf) -> Self {
        Self {
            path,
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

/// Options for sorting files and directories.
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum SortOption {
    /// Sort by name (alphabetically)
    Name,
    /// Sort by size (largest first)
    Size,
    /// Sort by modification time (newest first)
    Modified,
    /// Sort by file type (directories first, then by extension)
    Type,
}

/// Options for grouping files and directories.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum GroupByOption {
    /// No grouping (default)
    None,
    /// Group by parent folder
    Folder,
    /// Group by all parent folders
    AllFolders,
    /// Group by file extension
    Extension,
    /// Group by file permissions
    Permissions,
    /// Group by executable status
    Executable,
    /// Group by name prefix (requires pattern)
    NameStartsWith,
    /// Group by name substring (requires pattern)
    NameContains,
    /// Group by name suffix (requires pattern)
    NameEndsWith,
}

/// Options for output format.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum OutputFormat {
    /// Text format (default)
    Text,
    /// Table format
    Table,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn setup_test_dir() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let mut file = fs::File::create(file_path).unwrap();
        write!(file, "test content").unwrap();
        dir
    }

    #[test]
    fn test_args_validate_valid_path() {
        let temp_dir = setup_test_dir();
        let args = Args::new_for_test(temp_dir.path().to_path_buf());
        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_args_validate_invalid_path() {
        let args = Args::new_for_test(PathBuf::from("/nonexistent/path"));
        assert!(args.validate().is_err());
    }

    #[test]
    fn test_args_validate_empty_pattern() {
        let temp_dir = setup_test_dir();
        let mut args = Args::new_for_test(temp_dir.path().to_path_buf());
        args.name_pattern = Some("".to_string());
        args.group_by = GroupByOption::NameStartsWith;
        assert!(args.validate().is_err());
    }

    #[test]
    fn test_args_validate_valid_pattern() {
        let temp_dir = setup_test_dir();
        let mut args = Args::new_for_test(temp_dir.path().to_path_buf());
        args.name_pattern = Some("test".to_string());
        args.group_by = GroupByOption::NameStartsWith;
        assert!(args.validate().is_ok());
    }

    #[test]
    fn test_args_validate_pattern_not_needed() {
        let temp_dir = setup_test_dir();
        let mut args = Args::new_for_test(temp_dir.path().to_path_buf());
        args.name_pattern = Some("test".to_string());
        args.group_by = GroupByOption::Extension;
        assert!(args.validate().is_ok());
    }
} 