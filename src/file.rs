//! File system operations for the file manager.
//! 
//! This module provides functionality for interacting with the file system, including
//! reading directory contents and file metadata.
//! 
//! # Examples
//! 
//! ```rust
//! use ls_rs::file::FileInfo;
//! use std::path::Path;
//! use std::fs::File;
//! use std::io::Write;
//! use tempfile::tempdir;
//! 
//! let dir = tempdir().unwrap();
//! let file_path = dir.path().join("example.txt");
//! let mut file = File::create(&file_path).unwrap();
//! file.write_all(b"test content").unwrap();
//! 
//! let file_info = FileInfo::from_path(&file_path).unwrap();
//! println!("File size: {}", file_info.size);
//! ```

use std::fs;
use std::path::Path;
use chrono::{DateTime, Local};
use std::os::unix::fs::MetadataExt;
use users::get_user_by_uid;
use crate::cli::{Args, SortOption};
use crate::error::{FileManagerError, Result};

/// Information about a file or directory.
/// 
/// This struct contains metadata about a file or directory, including its name,
/// size, modification time, permissions, and owner.
/// 
/// # Examples
/// 
/// ```rust
/// use ls_rs::file::FileInfo;
/// use std::path::PathBuf;
/// 
/// fn example() -> Result<(), Box<dyn std::error::Error>> {
///     let path = PathBuf::from("test.txt");
///     let file_info = FileInfo::from_path(&path)?;
///     println!("File name: {}", file_info.name);
///     Ok(())
/// }
/// ```
#[derive(Debug)]
pub struct FileInfo {
    /// The name of the file or directory
    pub name: String,
    /// The size of the file in bytes (0 for directories)
    pub size: u64,
    /// The last modification time
    pub modified: DateTime<Local>,
    /// Whether this is a directory
    pub is_dir: bool,
    /// The file permissions (Unix-style)
    pub permissions: u32,
    /// The owner of the file
    pub owner: String,
}

impl FileInfo {
    /// Creates a new `FileInfo` instance from a path.
    /// 
    /// # Arguments
    /// 
    /// * `path` - The path to get information about
    /// 
    /// # Returns
    /// 
    /// * `Result<FileInfo, FileManagerError>` - Returns the file information if successful, or an error if the path doesn't exist
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use ls_rs::file::FileInfo;
    /// use std::path::PathBuf;
    /// 
    /// fn example() -> Result<(), Box<dyn std::error::Error>> {
    ///     let path = PathBuf::from("test.txt");
    ///     let file_info = FileInfo::from_path(&path)?;
    ///     println!("File name: {}", file_info.name);
    ///     Ok(())
    /// }
    /// ```
    pub fn from_path(path: &Path) -> Result<Self> {
        let metadata = fs::metadata(path)
            .map_err(|e| FileManagerError::InvalidMetadata(format!("Failed to get metadata for {}: {}", path.display(), e)))?;
        
        let name = path.file_name()
            .ok_or_else(|| FileManagerError::InvalidMetadata(format!("Invalid file name for path: {}", path.display())))?
            .to_string_lossy()
            .to_string();
            
        let size = if metadata.is_file() { metadata.len() } else { 0 };
        let modified = DateTime::from(metadata.modified()
            .map_err(|e| FileManagerError::InvalidMetadata(format!("Failed to get modification time for {}: {}", path.display(), e)))?);
        let is_dir = metadata.is_dir();
        let permissions = metadata.mode() & 0o777;
        let owner = get_user_by_uid(metadata.uid())
            .map(|u| u.name().to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        Ok(FileInfo {
            name,
            size,
            modified,
            is_dir,
            permissions,
            owner,
        })
    }
}

/// Check if a file should be included based on its name and the show_hidden flag.
/// 
/// # Arguments
/// 
/// * `name` - The name of the file
/// * `show_hidden` - Whether to show hidden files
/// 
/// # Returns
/// 
/// * `bool` - True if the file should be included, false otherwise
fn should_include_file(name: &str, show_hidden: bool) -> bool {
    show_hidden || !name.starts_with('.')
}

/// Sort a vector of FileInfo instances based on the specified sort option.
/// 
/// # Arguments
/// 
/// * `files` - The vector of FileInfo instances to sort
/// * `sort_by` - The sort option to use
fn sort_files(files: &mut Vec<FileInfo>, sort_by: &SortOption) {
    match sort_by {
        SortOption::Name => {
            files.sort_by(|a, b| {
                if a.is_dir != b.is_dir {
                    b.is_dir.cmp(&a.is_dir)
                } else {
                    a.name.to_lowercase().cmp(&b.name.to_lowercase())
                }
            });
        }
        SortOption::Size => {
            files.sort_by(|a, b| {
                if a.is_dir != b.is_dir {
                    b.is_dir.cmp(&a.is_dir)
                } else {
                    b.size.cmp(&a.size)
                }
            });
        }
        SortOption::Modified => {
            files.sort_by(|a, b| {
                if a.is_dir != b.is_dir {
                    b.is_dir.cmp(&a.is_dir)
                } else {
                    b.modified.cmp(&a.modified)
                }
            });
        }
        SortOption::Type => {
            files.sort_by(|a, b| {
                if a.is_dir != b.is_dir {
                    b.is_dir.cmp(&a.is_dir)
                } else {
                    let a_ext = Path::new(&a.name).extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("");
                    let b_ext = Path::new(&b.name).extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("");
                    a_ext.cmp(b_ext)
                }
            });
        }
    }
}

/// List files and directories recursively.
/// 
/// This function recursively traverses directories and collects information about
/// all files and directories encountered.
/// 
/// # Arguments
/// 
/// * `path` - The path to start listing from
/// * `args` - The command line arguments
/// 
/// # Returns
/// 
/// * `Result<Vec<FileInfo>, FileManagerError>` - Returns a vector of FileInfo instances
///   on success, or an error if a directory cannot be read.
pub fn list_directory_recursive(path: &Path, args: &Args) -> Result<Vec<FileInfo>> {
    let mut all_files = Vec::new();
    let entries = fs::read_dir(path)?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if let Ok(file_info) = FileInfo::from_path(&path) {
                if should_include_file(&file_info.name, args.show_hidden) {
                    all_files.push(file_info);
                    
                    if args.recursive && path.is_dir() {
                        let mut sub_files = list_directory_recursive(&path, args)?;
                        all_files.append(&mut sub_files);
                    }
                }
            }
        }
    }

    sort_files(&mut all_files, &args.sort_by);
    Ok(all_files)
}

/// List files and directories.
/// 
/// This function lists files and directories in the specified path, optionally
/// recursively if the recursive flag is set.
/// 
/// # Arguments
/// 
/// * `args` - The command line arguments
/// 
/// # Returns
/// 
/// * `Result<Vec<FileInfo>, FileManagerError>` - Returns a vector of FileInfo instances
///   on success, or an error if the directory cannot be read.
pub fn list_directory(args: &Args) -> Result<Vec<FileInfo>> {
    let path = Path::new(&args.path);
    if !path.exists() {
        return Err(FileManagerError::PathNotFound(args.path.clone()));
    }

    if args.recursive {
        list_directory_recursive(path, args)
    } else {
        let entries = fs::read_dir(path)?;
        let mut files = Vec::new();

        for entry in entries {
            if let Ok(entry) = entry {
                if let Ok(file_info) = FileInfo::from_path(&entry.path()) {
                    if should_include_file(&file_info.name, args.show_hidden) {
                        files.push(file_info);
                    }
                }
            }
        }

        sort_files(&mut files, &args.sort_by);
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;
    use std::os::unix::fs::PermissionsExt;

    fn setup_test_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    fn create_test_file(dir: &Path, name: &str, content: &str, permissions: u32) -> std::io::Result<()> {
        let path = dir.join(name);
        let mut file = File::create(&path)?;
        file.write_all(content.as_bytes())?;
        fs::set_permissions(&path, fs::Permissions::from_mode(permissions))?;
        Ok(())
    }

    fn create_test_dir(dir: &Path, name: &str, permissions: u32) -> std::io::Result<()> {
        let path = dir.join(name);
        fs::create_dir(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(permissions))?;
        Ok(())
    }

    #[test]
    fn test_file_info_from_regular_file() -> std::io::Result<()> {
        let temp_dir = setup_test_dir();
        create_test_file(temp_dir.path(), "test.txt", "content", 0o644)?;

        let file_path = temp_dir.path().join("test.txt");
        let file_info = FileInfo::from_path(&file_path).unwrap();

        assert_eq!(file_info.name, "test.txt");
        assert_eq!(file_info.size, 7); // "content".len()
        assert!(!file_info.is_dir);
        assert_eq!(file_info.permissions, 0o644);

        Ok(())
    }

    #[test]
    fn test_file_info_from_directory() -> std::io::Result<()> {
        let temp_dir = setup_test_dir();
        create_test_dir(temp_dir.path(), "testdir", 0o755)?;

        let dir_path = temp_dir.path().join("testdir");
        let file_info = FileInfo::from_path(&dir_path).unwrap();

        assert_eq!(file_info.name, "testdir");
        assert_eq!(file_info.size, 0);
        assert!(file_info.is_dir);
        assert_eq!(file_info.permissions, 0o755);

        Ok(())
    }

    #[test]
    fn test_file_info_from_nonexistent_path() {
        let temp_dir = setup_test_dir();
        let nonexistent = temp_dir.path().join("nonexistent");
        assert!(FileInfo::from_path(&nonexistent).is_err());
    }

    #[test]
    fn test_file_info_from_empty_file() -> std::io::Result<()> {
        let temp_dir = setup_test_dir();
        create_test_file(temp_dir.path(), "empty.txt", "", 0o644)?;

        let file_path = temp_dir.path().join("empty.txt");
        let file_info = FileInfo::from_path(&file_path).unwrap();

        assert_eq!(file_info.name, "empty.txt");
        assert_eq!(file_info.size, 0);
        assert!(!file_info.is_dir);

        Ok(())
    }

    #[test]
    fn test_file_info_with_special_permissions() -> std::io::Result<()> {
        let temp_dir = setup_test_dir();
        create_test_file(temp_dir.path(), "executable.sh", "#!/bin/sh", 0o755)?;

        let file_path = temp_dir.path().join("executable.sh");
        let file_info = FileInfo::from_path(&file_path).unwrap();

        assert_eq!(file_info.permissions, 0o755);
        assert!(!file_info.is_dir);

        Ok(())
    }
}