//! CLI module stub for testing.

use std::path::PathBuf;
use clap::Parser;
use crate::error::Result;

/// Command-line arguments.
#[derive(Parser, Debug, Clone)]
pub struct Args {
    /// Path to list
    pub path: PathBuf,
    
    /// Show hidden files
    pub show_hidden: bool,
    
    /// Use long view
    pub long_view: bool,
    
    /// Sort option
    pub sort_by: SortOption,
    
    /// Recursive listing
    pub recursive: bool,
    
    /// Show totals
    pub show_total: bool,
    
    /// Group by option
    pub group_by: GroupByOption,
    
    /// Name pattern
    pub name_pattern: Option<String>,
    
    /// Output format
    pub output_format: OutputFormat,
}

impl Args {
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl Default for Args {
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

/// Sort options.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum SortOption {
    Name,
    Size,
    Modified,
    Type,
}

/// Group by options.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum GroupByOption {
    None,
    Folder,
    AllFolders,
    Extension,
    Permissions,
    Executable,
    NameStartsWith,
    NameContains,
    NameEndsWith,
}

/// Output format options.
#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Table,
} 