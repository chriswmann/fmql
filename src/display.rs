//! Display formatting for the file manager.
//! 
//! This module provides functionality for formatting file information for display,
//! including file sizes, permissions, and grouping options.
//! 
//! # Examples
//! 
//! ```rust
//! use ls_rs::display::{format_size, format_permissions};
//! 
//! let size = format_size(1024);
//! assert_eq!(size, "1.0 KB");
//! 
//! let perms = format_permissions(0o755);
//! assert_eq!(perms, "rwxr-xr-x");
//! ```

use std::collections::HashMap;
use std::path::Path;
use crate::cli::{Args, GroupByOption, OutputFormat};
use crate::file::FileInfo;

pub fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size < KB {
        format!("{} B", size)
    } else if size < MB {
        format!("{:.1} KB", size as f64 / KB as f64)
    } else if size < GB {
        format!("{:.1} MB", size as f64 / MB as f64)
    } else {
        format!("{:.1} GB", size as f64 / GB as f64)
    }
}

pub fn format_permissions(permissions: u32) -> String {
    let mut perms = String::with_capacity(10);
    perms.push(if permissions & 0o400 != 0 { 'r' } else { '-' });
    perms.push(if permissions & 0o200 != 0 { 'w' } else { '-' });
    perms.push(if permissions & 0o100 != 0 { 'x' } else { '-' });
    perms.push(if permissions & 0o040 != 0 { 'r' } else { '-' });
    perms.push(if permissions & 0o020 != 0 { 'w' } else { '-' });
    perms.push(if permissions & 0o010 != 0 { 'x' } else { '-' });
    perms.push(if permissions & 0o004 != 0 { 'r' } else { '-' });
    perms.push(if permissions & 0o002 != 0 { 'w' } else { '-' });
    perms.push(if permissions & 0o001 != 0 { 'x' } else { '-' });
    perms
}

fn display_file_short(file: &FileInfo) {
    let dir_marker = if file.is_dir { "/" } else { "" };
    print!("{}{}  ", file.name, dir_marker);
}

fn display_file_long(file: &FileInfo) {
    let perms = format_permissions(file.permissions);
    let size = format_size(file.size);
    let modified = file.modified.format("%Y-%m-%d %H:%M").to_string();
    let dir_marker = if file.is_dir { "/" } else { "" };
    
    println!("{:<10} {:<8} {:<8} {:<19} {}{}",
        perms,
        file.owner,
        size,
        modified,
        file.name,
        dir_marker
    );
}

#[derive(Debug, Default)]
struct GroupStats {
    total_size: u64,
    file_count: usize,
    dir_count: usize,
}

impl GroupStats {
    fn new() -> Self {
        Self::default()
    }

    fn add_file(&mut self, size: u64) {
        self.total_size += size;
        self.file_count += 1;
    }

    fn add_directory(&mut self) {
        self.dir_count += 1;
    }
}

pub struct DisplayOptions<'a> {
    files: &'a [FileInfo],
    args: &'a Args,
}

impl<'a> DisplayOptions<'a> {
    pub fn new(files: &'a [FileInfo], args: &'a Args) -> Self {
        Self { files, args }
    }

    pub fn display(&self) {
        self.display_files();
        if self.args.show_total {
            self.display_totals();
        }
    }

    fn display_files(&self) {
        let files_to_display = if let Some(pattern) = &self.args.name_pattern {
            match self.args.group_by {
                GroupByOption::NameStartsWith | GroupByOption::NameContains | GroupByOption::NameEndsWith => {
                    self.files.iter()
                        .filter(|file| {
                            let name_lower = file.name.to_lowercase();
                            let pattern_lower = pattern.to_lowercase();
                            match self.args.group_by {
                                GroupByOption::NameStartsWith => name_lower.starts_with(&pattern_lower),
                                GroupByOption::NameContains => name_lower.contains(&pattern_lower),
                                GroupByOption::NameEndsWith => name_lower.ends_with(&pattern_lower),
                                _ => true,
                            }
                        })
                        .collect::<Vec<_>>()
                }
                _ => self.files.iter().collect(),
            }
        } else {
            self.files.iter().collect()
        };

        if self.args.long_view {
            for file in files_to_display {
                display_file_long(file);
            }
        } else {
            for file in files_to_display {
                display_file_short(file);
            }
            println!(); // Add newline at the end of short view
        }
    }

    fn display_totals(&self) {
        if self.args.group_by == GroupByOption::None {
            self.display_simple_totals();
        } else {
            let groups = self.calculate_grouped_totals(self.files);
            display_grouped_totals(&groups, &self.args.output_format);
        }
    }

    fn display_simple_totals(&self) {
        let stats = self.calculate_total_stats();
        match self.args.output_format {
            OutputFormat::Text => {
                println!("\nTotal: {} files, {} directories, total size: {}", 
                    stats.file_count, stats.dir_count, format_size(stats.total_size));
            }
            OutputFormat::Table => {
                println!("\nSummary:");
                println!("{:<10} {:<10} {:<10}", "Files", "Dirs", "Size");
                println!("{:-<10} {:-<10} {:-<10}", "", "", "");
                println!("{:<10} {:<10} {:<10}", 
                    stats.file_count, stats.dir_count, format_size(stats.total_size));
            }
        }
    }

    fn calculate_total_stats(&self) -> GroupStats {
        let mut stats = GroupStats::default();
        for file in self.files {
            if file.is_dir {
                stats.add_directory();
            } else {
                stats.add_file(file.size);
            }
        }
        stats
    }

    fn calculate_grouped_totals(&self, files: &[FileInfo]) -> HashMap<String, GroupStats> {
        let mut grouped_totals = HashMap::new();

        for file in files {
            let group = match self.args.group_by {
                GroupByOption::None => "all".to_string(),
                GroupByOption::Folder => get_folder_name(&file.name),
                GroupByOption::AllFolders => get_folder_name(&file.name),
                GroupByOption::Extension => get_file_extension(&file.name),
                GroupByOption::Permissions => get_permissions_group(file.permissions),
                GroupByOption::Executable => get_executable_group(file.permissions),
                GroupByOption::NameStartsWith | GroupByOption::NameContains | GroupByOption::NameEndsWith => {
                    if let Some(pattern) = &self.args.name_pattern {
                        get_name_group(&file.name, pattern, &self.args.group_by)
                    } else {
                        "no pattern".to_string()
                    }
                }
            };

            let stats = grouped_totals.entry(group).or_insert_with(GroupStats::new);
            if file.is_dir {
                stats.add_directory();
            } else {
                stats.add_file(file.size);
            }
        }

        grouped_totals
    }
}

fn get_file_extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_else(|| "no extension".to_string())
}

fn get_folder_name(path: &str) -> String {
    Path::new(path)
        .parent()
        .and_then(|p| p.to_str())
        .map(|p| if p.is_empty() { "." } else { p })
        .unwrap_or(".")
        .to_string()
}

fn display_grouped_totals_text(groups: &HashMap<String, GroupStats>) {
    println!("\nGrouped Totals:");
    for (group, stats) in groups {
        println!("{}:", group);
        println!("  Files: {}, Directories: {}, Total Size: {}", 
            stats.file_count, 
            stats.dir_count, 
            format_size(stats.total_size));
    }
}

fn display_grouped_totals_table(groups: &HashMap<String, GroupStats>) {
    // Calculate column widths
    let mut max_group_width = 0;
    let mut max_files_width = 0;
    let mut max_dirs_width = 0;
    let mut max_size_width = 0;

    for (group, stats) in groups {
        max_group_width = max_group_width.max(group.len());
        max_files_width = max_files_width.max(stats.file_count.to_string().len());
        max_dirs_width = max_dirs_width.max(stats.dir_count.to_string().len());
        max_size_width = max_size_width.max(format_size(stats.total_size).len());
    }

    // Add padding
    max_group_width += 2;
    max_files_width += 2;
    max_dirs_width += 2;
    max_size_width += 2;

    // Print header
    println!("\nGrouped Totals:");
    println!("{:<group_width$} {:<files_width$} {:<dirs_width$} {:<size_width$}",
        "Group",
        "Files",
        "Dirs",
        "Size",
        group_width = max_group_width.max(5),
        files_width = max_files_width.max(5),
        dirs_width = max_dirs_width.max(5),
        size_width = max_size_width.max(5));

    // Print separator
    println!("{:-<group_width$} {:-<files_width$} {:-<dirs_width$} {:-<size_width$}",
        "", "", "", "",
        group_width = max_group_width.max(5),
        files_width = max_files_width.max(5),
        dirs_width = max_dirs_width.max(5),
        size_width = max_size_width.max(5));

    // Print rows
    for (group, stats) in groups {
        println!("{:<group_width$} {:<files_width$} {:<dirs_width$} {:<size_width$}",
            group,
            stats.file_count,
            stats.dir_count,
            format_size(stats.total_size),
            group_width = max_group_width.max(5),
            files_width = max_files_width.max(5),
            dirs_width = max_dirs_width.max(5),
            size_width = max_size_width.max(5));
    }
}

fn display_grouped_totals(groups: &HashMap<String, GroupStats>, format: &OutputFormat) {
    match format {
        OutputFormat::Text => display_grouped_totals_text(groups),
        OutputFormat::Table => display_grouped_totals_table(groups),
    }
}

pub fn display_file_list(files: &[FileInfo], args: &Args) {
    let display = DisplayOptions::new(files, args);
    display.display();
}

/// Get the permissions group for a file based on its permissions.
/// 
/// # Arguments
/// 
/// * `permissions` - The file permissions
/// 
/// # Returns
/// 
/// A string representing the permissions group
fn get_permissions_group(permissions: u32) -> String {
    match permissions {
        0o777 => "full access".to_string(),
        0o755 => "read & execute".to_string(),
        0o750 => "read & execute (owner)".to_string(),
        0o644 => "read only".to_string(),
        0o640 => "read only (owner)".to_string(),
        0o600 => "owner only".to_string(),
        _ => "custom".to_string(),
    }
}

/// Get the executable group for a file based on its permissions.
/// 
/// # Arguments
/// 
/// * `permissions` - The file permissions
/// 
/// # Returns
/// 
/// A string indicating if the file is executable
fn get_executable_group(permissions: u32) -> String {
    if permissions & 0o111 != 0 {
        "Executable".to_string()
    } else {
        "Not executable".to_string()
    }
}

/// Get the name group for a file based on the pattern and group option.
/// 
/// # Arguments
/// 
/// * `name` - The file name
/// * `pattern` - The pattern to match against
/// * `group_by` - The group option
/// 
/// # Returns
/// 
/// A string indicating if the file matches the pattern
fn get_name_group(name: &str, pattern: &str, group_by: &GroupByOption) -> String {
    let name_lower = name.to_lowercase();
    let pattern_lower = pattern.to_lowercase();

    match group_by {
        GroupByOption::NameStartsWith => {
            if name_lower.starts_with(&pattern_lower) {
                "Matches".to_string()
            } else {
                "Does not match".to_string()
            }
        }
        GroupByOption::NameContains => {
            if name_lower.contains(&pattern_lower) {
                "Matches".to_string()
            } else {
                "Does not match".to_string()
            }
        }
        GroupByOption::NameEndsWith => {
            if name_lower.ends_with(&pattern_lower) {
                "Matches".to_string()
            } else {
                "Does not match".to_string()
            }
        }
        _ => "invalid group option".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};
    use std::path::PathBuf;

    fn create_test_file(name: &str, size: u64, is_dir: bool, permissions: u32) -> FileInfo {
        FileInfo {
            name: name.to_string(),
            size,
            modified: Local.timestamp_opt(1234567890, 0).unwrap(),
            is_dir,
            permissions,
            owner: "test_user".to_string(),
        }
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_size(1024 * 1024 * 1024 * 2), "2.0 GB");
    }

    #[test]
    fn test_format_permissions() {
        assert_eq!(format_permissions(0o000), "---------");
        assert_eq!(format_permissions(0o777), "rwxrwxrwx");
        assert_eq!(format_permissions(0o644), "rw-r--r--");
        assert_eq!(format_permissions(0o755), "rwxr-xr-x");
    }

    #[test]
    fn test_get_file_extension() {
        assert_eq!(get_file_extension("test.txt"), "txt");
        assert_eq!(get_file_extension("test"), "no extension");
        assert_eq!(get_file_extension(".hidden"), "no extension");
        assert_eq!(get_file_extension("test.TXT"), "txt");
        assert_eq!(get_file_extension("test.tar.gz"), "gz");
    }

    #[test]
    fn test_get_folder_name() {
        assert_eq!(get_folder_name("test.txt"), ".");
        assert_eq!(get_folder_name("dir/test.txt"), "dir");
        assert_eq!(get_folder_name("dir/subdir/test.txt"), "dir/subdir");
        assert_eq!(get_folder_name("/"), ".");
    }

    #[test]
    fn test_group_stats() {
        let mut stats = GroupStats::default();
        assert_eq!(stats.total_size, 0);
        assert_eq!(stats.file_count, 0);
        assert_eq!(stats.dir_count, 0);

        stats.add_file(100);
        assert_eq!(stats.total_size, 100);
        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.dir_count, 0);

        stats.add_directory();
        assert_eq!(stats.total_size, 100);
        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.dir_count, 1);
    }

    #[test]
    fn test_calculate_grouped_totals() {
        let files = vec![
            create_test_file("test1.txt", 100, false, 0o644),
            create_test_file("test2.txt", 200, false, 0o644),
            create_test_file("dir1", 0, true, 0o755),
            create_test_file("test.rs", 300, false, 0o644),
        ];

        let mut args = Args::new_for_test(PathBuf::from("."));
        args.group_by = GroupByOption::Extension;

        let display = DisplayOptions::new(&files, &args);
        let groups = display.calculate_grouped_totals(&files);

        assert_eq!(groups.len(), 3); // txt, rs, directories
        assert_eq!(groups.get("txt").unwrap().file_count, 2);
        assert_eq!(groups.get("rs").unwrap().file_count, 1);
        assert_eq!(groups.get("no extension").unwrap().dir_count, 1);
    }

    #[test]
    fn test_name_based_grouping() {
        let files = vec![
            create_test_file("test1.txt", 100, false, 0o644),
            create_test_file("other.txt", 200, false, 0o644),
            create_test_file("test2.rs", 300, false, 0o644),
        ];

        let mut args = Args::new_for_test(PathBuf::from("."));
        args.group_by = GroupByOption::NameStartsWith;
        args.name_pattern = Some("test".to_string());

        let display = DisplayOptions::new(&files, &args);
        let groups = display.calculate_grouped_totals(&files);

        assert_eq!(groups.len(), 2);
        assert!(groups.contains_key("Matches"));
        assert!(groups.contains_key("Does not match"));
    }

    #[test]
    fn test_executable_grouping() {
        let files = vec![
            create_test_file("script.sh", 100, false, 0o755),
            create_test_file("readme.txt", 200, false, 0o644),
        ];

        let mut args = Args::new_for_test(PathBuf::from("."));
        args.group_by = GroupByOption::Executable;

        let display = DisplayOptions::new(&files, &args);
        let groups = display.calculate_grouped_totals(&files);

        assert_eq!(groups.len(), 2);
        assert!(groups.contains_key("Executable"));
        assert!(groups.contains_key("Not executable"));
    }

    #[test]
    fn test_case_insensitive_name_grouping() {
        let files = vec![
            create_test_file("test1.RS", 100, false, 0o644),
            create_test_file("other.txt", 200, false, 0o644),
            create_test_file("test2.rs", 300, false, 0o644),
            create_test_file("TEST3.Rs", 400, false, 0o644),
        ];

        let mut args = Args::new_for_test(PathBuf::from("."));
        args.group_by = GroupByOption::NameContains;
        args.name_pattern = Some(".rs".to_string());

        let display = DisplayOptions::new(&files, &args);
        let groups = display.calculate_grouped_totals(&files);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups.get("Matches").unwrap().file_count, 3);
        assert_eq!(groups.get("Does not match").unwrap().file_count, 1);
    }
} 