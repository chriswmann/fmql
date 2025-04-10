//! File module stub for testing.

use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use crate::cli::Args;
use crate::error::{FileManagerError, Result};
use std::fs;
use walkdir::WalkDir;
use glob::Pattern;

/// File information structure.
#[derive(Debug, Clone)]
pub struct FileInfo {
    /// The file path.
    pub path: PathBuf,
    /// The file name.
    pub name: String,
    /// The file size in bytes.
    pub size: u64,
    /// Whether the file is a directory.
    pub is_dir: bool,
    /// Whether the file is a symlink.
    pub is_symlink: bool,
    /// The file permissions.
    pub permissions: u32,
    /// The file modification time.
    pub modified: DateTime<Utc>,
}

impl FileInfo {
    /// Create a new FileInfo from a path.
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

/// List files in a directory.
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