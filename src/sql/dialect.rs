//! Custom SQL dialect for file management operations.
//!
//! This module defines a custom SQL dialect that supports file-specific syntax
//! and keywords, building upon the sqlparser crate. The dialect is specifically
//! designed to handle file paths, file attributes, and file-specific operations
//! that regular SQL dialects might not support.
//!
//! # Features
//!
//! - Support for file system paths as identifiers (including ~, /, ., etc.)
//! - Recognition of file-specific attributes (size, modification time, etc.)
//! - Support for file path patterns and wildcards in queries
//!
//! # Examples
//!
//! This dialect allows SQL queries like:
//!
//! ```sql
//! -- Query with file paths
//! SELECT * FROM ~/Documents WHERE name LIKE '%.txt'
//!
//! -- Query with file attributes
//! SELECT * FROM /var/log WHERE size > 1000000 AND modified < '2023-01-01'
//!
//! -- Windows-style paths
//! SELECT * FROM C:/Users/Documents WHERE extension = 'pdf'
//! ```

use sqlparser::dialect::{Dialect, GenericDialect};

/// A custom SQL dialect for file management operations.
///
/// This dialect extends the GenericDialect with specific syntax elements
/// for file operations, allowing SQL queries to reference file paths,
/// use file-specific attributes, and perform file management operations.
///
/// # Examples
///
/// The dialect can be used to parse SQL queries containing file paths:
///
/// ```no_run
/// use ls_rs::sql::dialect::FileDialect;
/// use sqlparser::parser::Parser;
///
/// let sql = "SELECT * FROM ~/Documents WHERE name LIKE '%.pdf'";
/// let dialect = FileDialect::new();
/// let ast = Parser::parse_sql(&dialect, sql).unwrap();
/// // Process the parsed AST...
/// ```
#[derive(Debug, Default)]
pub struct FileDialect {
    /// Base dialect for standard SQL parsing
    generic: GenericDialect,
}

impl FileDialect {
    /// Creates a new FileDialect instance with default settings.
    ///
    /// # Examples
    ///
    /// ```
    /// use ls_rs::sql::dialect::FileDialect;
    ///
    /// let dialect = FileDialect::new();
    /// // Use dialect for parsing SQL queries...
    /// ```
    pub fn new() -> Self {
        Self {
            generic: GenericDialect::default(),
        }
    }
}

impl Dialect for FileDialect {
    /// Determines whether a character can start an identifier.
    ///
    /// This implementation extends the generic SQL dialect to allow
    /// common characters used in file paths like '~', '/', and '.'
    /// to be valid at the start of identifiers.
    ///
    /// # Arguments
    /// * `ch` - The character to check
    ///
    /// # Returns
    /// * `bool` - True if the character can start an identifier
    fn is_identifier_start(&self, ch: char) -> bool {
        // Allow ~ for home directory paths and characters that are valid in file paths
        ch == '~' || ch == '/' || ch == '.' || ch == '_' || ch == '-' || 
        self.generic.is_identifier_start(ch)
    }

    /// Determines whether a character can be part of an identifier.
    ///
    /// This implementation extends the generic SQL dialect to allow
    /// characters commonly found in file paths like '/', '\', and ':'
    /// to be valid within identifiers.
    ///
    /// # Arguments
    /// * `ch` - The character to check
    ///
    /// # Returns
    /// * `bool` - True if the character can be part of an identifier
    fn is_identifier_part(&self, ch: char) -> bool {
        // Allow characters that can be part of file paths
        ch == '/' || ch == '\\' || ch == '.' || ch == '_' || ch == '-' || ch == '~' || 
        ch == ':' || // For Windows paths (C:)
        self.generic.is_identifier_part(ch)
    }

    /// Determines whether a character can start a delimited identifier.
    ///
    /// This implementation defers to the generic SQL dialect.
    ///
    /// # Arguments
    /// * `ch` - The character to check
    ///
    /// # Returns
    /// * `bool` - True if the character can start a delimited identifier
    fn is_delimited_identifier_start(&self, ch: char) -> bool {
        self.generic.is_delimited_identifier_start(ch)
    }
} 