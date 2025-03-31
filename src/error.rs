//! Custom error types for the file manager.
//! 
//! This module provides custom error types and a Result type alias for the file manager.
//! 
//! # Examples
//! 
//! ```rust
//! use ls_rs::error::{Result, FileManagerError};
//! use std::path::PathBuf;
//! 
//! fn example() -> Result<()> {
//!     let path = PathBuf::from("nonexistent.txt");
//!     if !path.exists() {
//!         return Err(FileManagerError::PathNotFound(path));
//!     }
//!     Ok(())
//! }
//! ```

use std::path::PathBuf;
use thiserror::Error;

/// Custom error type for file manager operations.
/// 
/// This enum represents various error conditions that can occur while using the file manager.
/// 
/// # Examples
/// 
/// ```rust
/// use ls_rs::error::FileManagerError;
/// use std::path::PathBuf;
/// 
/// let path = PathBuf::from("nonexistent.txt");
/// let error = FileManagerError::PathNotFound(path);
/// ```
#[derive(Error, Debug)]
pub enum FileManagerError {
    /// An IO error occurred
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// The specified path was not found
    #[error("Path not found: {0}")]
    PathNotFound(PathBuf),

    /// An invalid pattern was provided
    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),

    /// An invalid sort option was provided
    #[error("Invalid sort option: {0}")]
    InvalidSortOption(String),

    /// An invalid group option was provided
    #[error("Invalid group option: {0}")]
    InvalidGroupOption(String),

    /// A pattern is required for the specified group option
    #[error("Pattern required for group option: {0}")]
    PatternRequired(String),

    /// Invalid file metadata was encountered
    #[error("Invalid file metadata: {0}")]
    InvalidMetadata(String),
}

impl From<FileManagerError> for std::io::Error {
    fn from(error: FileManagerError) -> Self {
        match error {
            FileManagerError::Io(io_error) => io_error,
            _ => std::io::Error::new(
                std::io::ErrorKind::Other,
                error.to_string(),
            ),
        }
    }
}

/// A type alias for the Result type using FileManagerError.
/// 
/// This type alias makes it easier to use the custom error type throughout the codebase.
/// 
/// # Examples
/// 
/// ```no_run
/// use ls_rs::error::Result;
/// 
/// fn my_function() -> Result<()> {
///     // ...
///     Ok(())
/// }
/// ```
pub type Result<T> = std::result::Result<T, FileManagerError>; 