//! Error module stub for testing.

use std::path::PathBuf;
use thiserror::Error;

/// Error type for file operations.
#[derive(Error, Debug)]
pub enum FileManagerError {
    /// Error when a path doesn't exist.
    #[error("Path not found: {0}")]
    PathNotFound(PathBuf),
    
    /// Error when a pattern is invalid.
    #[error("Invalid pattern: {0}")]
    InvalidPattern(String),
    
    /// Error from IO operations.
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type for file operations.
pub type Result<T> = std::result::Result<T, FileManagerError>; 