//! File management module that provides core functionality for accessing and manipulating files.
//!
//! This module contains the structures and functions needed to represent file information
//! and to perform operations like listing directories with various filtering and sorting options.
//!
//! # Examples
//!
//! Basic usage for listing a directory:
//!
//! ```no_run
//! use fmql::cli::{Args, SortOption, GroupByOption, OutputFormat};
//! use fmql::file::list_directory;
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
//! for file in files {
//!     println!("{} ({} bytes)", file.name, file.size);
//! }
//! ```

use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use crate::cli::Args;
use crate::error::{FileManagerError, Result};
use walkdir::WalkDir;
use glob::Pattern;

/// Represents detailed information about a file or directory.
///
/// This structure encapsulates all relevant metadata about a file,
/// including its path, name, size, type (directory/file/symlink),
/// permissions, and modification time.
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// The absolute file path.
    pub path: PathBuf,
    /// The file name (without path).
    pub name: String,
    /// The file size in bytes.
    pub size: u64,
    /// Whether the entry is a directory.
    pub is_dir: bool,
    /// Whether the entry is a symbolic link.
    pub is_symlink: bool,
    /// The file permissions in octal format (e.g., 0o644).
    pub permissions: u32,
    /// The file's last modification time.
    pub modified: DateTime<Utc>,
}

impl FileInfo {
    /// Creates a new `FileInfo` instance from a filesystem path.
    ///
    /// This method attempts to read the file's metadata and populate
    /// a `FileInfo` structure with all relevant information.
    ///
    /// # Arguments
    /// * `path` - A reference to the filesystem path to get information about
    ///
    /// # Returns
    /// * `Result<FileInfo>` - A result containing either the file information or an error
    ///
    /// # Errors
    /// Returns an error if:
    /// * The file doesn't exist
    /// * The metadata cannot be read due to permissions or other IO errors
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use fmql::file::FileInfo;
    /// use std::path::Path;
    ///
    /// // Get information about a file
    /// let path = Path::new("README.md");
    /// match FileInfo::from_path(path) {
    ///     Ok(info) => println!("File: {} is {} bytes", info.name, info.size),
    ///     Err(e) => eprintln!("Error: {}", e),
    /// }
    /// ```
    pub fn from_path(path: &Path) -> Result<Self> {
        let metadata = path.metadata().map_err(|e| {
            FileManagerError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to get metadata for {}: {}", path.display(), e),
            ))
        })?;
        
        let name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        
        let modified = metadata.modified()
            .map(|time| DateTime::<Utc>::from(time))
            .unwrap_or_else(|_| Utc::now());
        
        Ok(Self {
            path: path.to_path_buf(),
            name,
            size: metadata.len(),
            is_dir: metadata.is_dir(),
            is_symlink: metadata.file_type().is_symlink(),
            permissions: 0o644, // Dummy value
            modified,
        })
    }
}

/// Lists files in a directory based on the provided arguments.
///
/// This function walks through a directory (recursively if specified),
/// applies filters, and returns a list of file information.
///
/// # Arguments
/// * `args` - A reference to `Args` containing configuration options
///
/// # Returns
/// * `Result<Vec<FileInfo>>` - A result containing either a vector of file information or an error
///
/// # Filtering
/// The function applies the following filters based on the arguments:
/// * Hidden files (files starting with `.`) can be included or excluded
/// * Name patterns can be specified to match only certain files
/// * Recursive mode can traverse subdirectories
///
/// # Sorting
/// Files can be sorted by:
/// * Name
/// * Size
/// * Modification time
/// * File type/extension
///
/// # Grouping
/// Files can be grouped by:
/// * Folder (directories first)
/// * Extension
///
/// # Examples
///
/// ```no_run
/// use fmql::cli::{Args, SortOption, GroupByOption, OutputFormat};
/// use fmql::file::list_directory;
/// use std::path::PathBuf;
///
/// // List all text files recursively
/// let args = Args {
///     path: PathBuf::from("."),
///     long_view: true,
///     show_total: false,
///     recursive: true,
///     show_hidden: false,
///     group_by: GroupByOption::Extension,
///     name_pattern: Some("*.txt".to_string()),
///     sort_by: SortOption::Size,
///     output_format: OutputFormat::Text,
/// };
///
/// match list_directory(&args) {
///     Ok(files) => {
///         println!("Found {} text files:", files.len());
///         for file in files {
///             println!("{} ({} bytes)", file.name, file.size);
///         }
///     },
///     Err(e) => eprintln!("Error: {}", e),
/// }
/// ```
pub fn list_directory(args: &Args) -> Result<Vec<FileInfo>> {
    let mut files = Vec::new();
    
    // Determine if we should search recursively
    let walker = if args.recursive {
        WalkDir::new(&args.path).into_iter()
    } else {
        WalkDir::new(&args.path).max_depth(1).into_iter()
    };
    
    // Collect files based on arguments
    for entry in walker.filter_map(|e| e.ok()) {
        // Skip current directory entry
        if entry.path() == args.path {
            continue;
        }
        
        // Skip hidden files if not showing hidden
        if !args.show_hidden {
            if let Some(file_name) = entry.file_name().to_str() {
                if file_name.starts_with('.') {
                    continue;
                }
            }
        }
        
        // Check name pattern
        if let Some(pattern) = &args.name_pattern {
            if let Some(file_name) = entry.file_name().to_str() {
                if let Ok(glob_pattern) = Pattern::new(pattern) {
                    if !glob_pattern.matches(file_name) {
                        continue;
                    }
                }
            }
        }
        
        // Create FileInfo from entry
        if let Ok(file_info) = FileInfo::from_path(entry.path()) {
            files.push(file_info);
        }
    }
    
    // Sort files
    match args.sort_by {
        crate::cli::SortOption::Name => {
            files.sort_by(|a, b| a.name.cmp(&b.name));
        },
        crate::cli::SortOption::Size => {
            files.sort_by(|a, b| b.size.cmp(&a.size));
        },
        crate::cli::SortOption::Modified => {
            files.sort_by(|a, b| b.modified.cmp(&a.modified));
        },
        crate::cli::SortOption::Type => {
            files.sort_by(|a, b| {
                let a_ext = Path::new(&a.name).extension().and_then(|e| e.to_str()).unwrap_or("");
                let b_ext = Path::new(&b.name).extension().and_then(|e| e.to_str()).unwrap_or("");
                a_ext.cmp(b_ext)
            });
        },
    }
    
    // Group files if requested
    if args.group_by != crate::cli::GroupByOption::None {
        // For simplicity, we'll implement just directory grouping
        if args.group_by == crate::cli::GroupByOption::Folder {
            // Put directories first
            files.sort_by(|a, b| a.is_dir.cmp(&b.is_dir).reverse());
        } else if args.group_by == crate::cli::GroupByOption::Extension {
            // Group by extension
            files.sort_by(|a, b| {
                let a_ext = Path::new(&a.name).extension().and_then(|e| e.to_str()).unwrap_or("");
                let b_ext = Path::new(&b.name).extension().and_then(|e| e.to_str()).unwrap_or("");
                a_ext.cmp(b_ext)
            });
        }
    }
    
    Ok(files)
} 