//! Custom SQL dialect for file management operations.
//!
//! This module defines a custom SQL dialect that supports file-specific syntax
//! and keywords, building upon the sqlparser crate.

use sqlparser::dialect::{Dialect, GenericDialect};

/// A custom SQL dialect for file management operations.
///
/// This dialect extends the GenericDialect with specific keywords and syntax
/// for file operations, such as file paths, permissions, and attributes.
#[derive(Debug, Default)]
pub struct FileDialect {
    // Base dialect for standard SQL parsing
    generic: GenericDialect,
}

impl FileDialect {
    /// Creates a new FileDialect instance.
    pub fn new() -> Self {
        Self {
            generic: GenericDialect::default(),
        }
    }
}

impl Dialect for FileDialect {
    /// Returns true if the specified keyword is reserved in this dialect.
    fn is_identifier_start(&self, ch: char) -> bool {
        // Allow ~ for home directory paths and characters that are valid in file paths
        ch == '~' || ch == '/' || ch == '.' || ch == '_' || ch == '-' || 
        self.generic.is_identifier_start(ch)
    }

    /// Returns true if the specified character can be part of an identifier.
    fn is_identifier_part(&self, ch: char) -> bool {
        // Allow characters that can be part of file paths
        ch == '/' || ch == '\\' || ch == '.' || ch == '_' || ch == '-' || ch == '~' || 
        ch == ':' || // For Windows paths (C:)
        self.generic.is_identifier_part(ch)
    }

    /// Returns true if the given string is a reserved keyword in this dialect.
    fn is_delimited_identifier_start(&self, ch: char) -> bool {
        self.generic.is_delimited_identifier_start(ch)
    }
} 