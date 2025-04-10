//! Display module stub for testing.

use crate::file::FileInfo;
use crate::cli::Args;

/// Display a list of files.
pub fn display_file_list(files: &[FileInfo], args: &Args) {
    // Stub implementation
    for file in files {
        println!("{}", file.name);
    }
} 