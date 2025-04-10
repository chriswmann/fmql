//! Error handling module for the file manager.
//!
//! This module defines the error types and result type used throughout the application.
//! It provides a consistent error handling approach using the `thiserror` crate to
//! implement error types that are both user-friendly and programmatically useful.
//!
//! # Examples
//!
//! ```
//! use fmql::error::{Result, FileManagerError};
//! use std::path::PathBuf;
//!
//! fn example_function() -> Result<()> {
//!     // Simulate an error with a non-existent path
//!     let path = PathBuf::from("/path/does/not/exist");
//!     Err(FileManagerError::PathNotFound(path))
//! }
//!
//! // Handle the error
//! match example_function() {
//!     Ok(()) => println!("Success!"),
//!     Err(e) => eprintln!("Error: {}", e),
//! }
//! ```

use std::path::PathBuf;
use thiserror::Error;

/// Comprehensive error type for file manager operations.
///
/// This enum represents all possible errors that can occur during file operations,
/// including IO errors, invalid patterns, and missing paths.
///
/// # Examples
///
/// Creating and handling different error types:
///
/// ```
/// use fmql::error::FileManagerError;
/// use std::path::PathBuf;
///
/// // Creating a path not found error
/// let path_error = FileManagerError::PathNotFound(PathBuf::from("/missing/path"));
/// assert!(format!("{}", path_error).contains("Path not found"));
///
/// // Creating a pattern error
/// let pattern_error = FileManagerError::InvalidPattern("**/*.[".to_string());
/// assert!(format!("{}", pattern_error).contains("Invalid pattern"));
/// ```
#[derive(Error, Debug)]
pub enum FileManagerError {
    /// Error when a specified path doesn't exist or is inaccessible.
    ///
    /// This error occurs when trying to access a file or directory that
    /// either doesn't exist or cannot be accessed due to permissions.
    #[error("Path not found: {0}")]
    PathNotFound(PathBuf),
    
    /// Error when a pattern (e.g., glob or regex) is syntactically invalid.
    ///
    /// This error occurs when a user provides a pattern that cannot be parsed
    /// or is not valid according to the pattern syntax rules.
    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),
    
    /// Error from underlying IO operations.
    ///
    /// This wraps standard IO errors from the Rust standard library.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type used throughout the file manager.
///
/// This type is a specialized `Result` that uses `FileManagerError` as its error type,
/// making error handling more consistent throughout the application.
///
/// # Examples
///
/// ```
/// use fmql::error::{Result, FileManagerError};
///
/// fn sample_operation() -> Result<String> {
///     // Simulate success
///     Ok("Operation succeeded".to_string())
///     
///     // Or simulate failure
///     // Err(FileManagerError::InvalidPattern("*.[".to_string()))
/// }
/// ```
pub type Result<T> = std::result::Result<T, FileManagerError>; 