//! Error handling module for the file manager.
//!
//! This module defines the error types and result type used throughout the application.
//! It provides a consistent error handling approach using the `thiserror` crate to
//! implement error types that are both user-friendly and programmatically useful.
//!
//! # Examples

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
/// use fmql::error::FMQLError;
///
/// // Creating an IoError
/// let io_error = FMQLError::IoError(std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"));
/// assert!(format!("{}", io_error).contains("IO error"));
/// ```
#[derive(Error, Debug)]
pub enum FMQLError {
    /// Error when a specified path doesn't exist or is inaccessible.
    ///

    /// Error from underlying IO operations.
    ///
    /// This wraps standard IO errors from the Rust standard library.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}
