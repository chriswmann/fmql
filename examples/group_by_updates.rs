use chrono::{DateTime, TimeZone, Utc};
use fmql::sql::{execute_query, parse_sql};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;
use tempfile::{TempDir, tempdir};

/// Creates sample files with different creation dates in a temporary directory
fn setup_test_directory() -> TempDir {
    let dir = tempdir().expect("Failed to create temp directory");
    println!("Created temporary directory at: {}", dir.path().display());

    // Create different types of files with varying creation dates

    // Create a mix of document types from different "months"
    create_file_with_date(
        &dir,
        "january_report.txt",
        "January report content",
        0o644,
        Utc.with_ymd_and_hms(2023, 1, 15, 10, 0, 0).unwrap(),
    );

    create_file_with_date(
        &dir,
        "january_memo.doc",
        "January memo content",
        0o644,
        Utc.with_ymd_and_hms(2023, 1, 20, 14, 30, 0).unwrap(),
    );

    create_file_with_date(
        &dir,
        "february_report.txt",
        "February report content",
        0o644,
        Utc.with_ymd_and_hms(2023, 2, 10, 9, 15, 0).unwrap(),
    );

    create_file_with_date(
        &dir,
        "february_presentation.ppt",
        "Presentation slides",
        0o644,
        Utc.with_ymd_and_hms(2023, 2, 25, 16, 45, 0).unwrap(),
    );

    create_file_with_date(
        &dir,
        "march_budget.xlsx",
        "Q1 budget spreadsheet",
        0o644,
        Utc.with_ymd_and_hms(2023, 3, 5, 11, 20, 0).unwrap(),
    );

    create_file_with_date(
        &dir,
        "march_notes.txt",
        "Meeting notes",
        0o644,
        Utc.with_ymd_and_hms(2023, 3, 18, 13, 10, 0).unwrap(),
    );

    // Create some image files from different days
    create_file_with_date(
        &dir,
        "vacation1.jpg",
        &vec![0; 10000]
            .into_iter()
            .map(|_| b'X')
            .collect::<Vec<u8>>(),
        0o644,
        Utc.with_ymd_and_hms(2023, 7, 1, 10, 0, 0).unwrap(),
    );

    create_file_with_date(
        &dir,
        "vacation2.jpg",
        &vec![0; 12000]
            .into_iter()
            .map(|_| b'X')
            .collect::<Vec<u8>>(),
        0o644,
        Utc.with_ymd_and_hms(2023, 7, 1, 14, 30, 0).unwrap(),
    );

    create_file_with_date(
        &dir,
        "vacation3.jpg",
        &vec![0; 15000]
            .into_iter()
            .map(|_| b'X')
            .collect::<Vec<u8>>(),
        0o644,
        Utc.with_ymd_and_hms(2023, 7, 2, 9, 0, 0).unwrap(),
    );

    // Create files with different hour timestamps
    create_file_with_date(
        &dir,
        "morning_log.txt",
        "Morning log entry",
        0o644,
        Utc.with_ymd_and_hms(2023, 8, 15, 8, 0, 0).unwrap(),
    );

    create_file_with_date(
        &dir,
        "afternoon_log.txt",
        "Afternoon log entry",
        0o644,
        Utc.with_ymd_and_hms(2023, 8, 15, 14, 0, 0).unwrap(),
    );

    create_file_with_date(
        &dir,
        "evening_log.txt",
        "Evening log entry",
        0o644,
        Utc.with_ymd_and_hms(2023, 8, 15, 20, 0, 0).unwrap(),
    );

    // Create today's files
    let today = Utc::now();
    create_file_with_date(&dir, "today_note1.txt", "First note today", 0o644, today);

    // Sleep a bit to ensure different timestamps
    sleep(Duration::from_secs(2));

    let today = Utc::now();
    create_file_with_date(&dir, "today_note2.txt", "Second note today", 0o644, today);

    dir
}

/// Helper function to create a file with a specific modification date
fn create_file_with_date(
    dir: &TempDir,
    path: &str,
    content: &(impl AsRef<[u8]> + ?Sized),
    mode: u32,
    date: DateTime<Utc>,
) {
    let full_path = dir.path().join(path);

    // Create parent directories if needed
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).expect("Failed to create parent directory");
    }

    // Create and write to the file
    let mut file = File::create(&full_path).expect("Failed to create file");
    file.write_all(content.as_ref())
        .expect("Failed to write content");

    // Set file permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(mode);
        fs::set_permissions(&full_path, perms).expect("Failed to set permissions");
    }

    // Set the modification time
    #[cfg(unix)]
    {
        use filetime::FileTime;
        let secs = date.timestamp();
        let nsecs = 0;
        filetime::set_file_mtime(&full_path, FileTime::from_unix_time(secs, nsecs as u32))
            .expect("Failed to set modification time");
    }

    println!("Created file: {} with date: {}", full_path.display(), date);
}

fn main() {
    // Setup test directory with sample files
    let temp_dir = setup_test_directory();
    let dir_path = temp_dir.path().to_path_buf();

    println!("\n=== Renaming Files by Month ===");
    rename_by_month(&dir_path);

    println!("\n=== Organizing Files by Day ===");
    organize_by_day(&dir_path);

    println!("\n=== Prefixing Files by Hour ===");
    prefix_by_hour(&dir_path);

    println!("\n=== Files After Renaming Operations ===");
    list_all_files(&dir_path);

    // The temporary directory will be automatically cleaned up when temp_dir goes out of scope
    println!("\nTemporary directory will be cleaned up automatically");
}

/// Example of renaming files by prepending the month to their filename
fn rename_by_month(dir_path: &Path) {
    // SQL query to rename .txt files by prepending the month
    let query = format!(
        "UPDATE {} SET name = CONCAT(MONTH(modified), '_', name) WHERE extension = 'txt' AND NOT name LIKE '%\\_%'",
        dir_path.display()
    );
    println!("Query: {}", query);

    execute_sql_query(&query);
}

/// Example of organizing files by creating directories for each day and moving files
fn organize_by_day(dir_path: &Path) {
    // SQL query to organize jpg files into day-based directories
    let query = format!(
        "UPDATE {} SET path = CONCAT({}, '/day_', DAY(modified)) WHERE extension = 'jpg'",
        dir_path.display(),
        dir_path.display()
    );
    println!("Query: {}", query);

    // First, create the day directories
    let day_dirs = ["day_1", "day_2"];
    for day in &day_dirs {
        let day_path = dir_path.join(day);
        if !day_path.exists() {
            fs::create_dir(&day_path).expect("Failed to create day directory");
            println!("Created directory: {}", day_path.display());
        }
    }

    execute_sql_query(&query);
}

/// Example of prefixing files by the hour they were modified
fn prefix_by_hour(dir_path: &Path) {
    // SQL query to prefix log files with the hour they were created
    let query = format!(
        "UPDATE {} SET name = CONCAT(HOUR(modified), 'h_', name) WHERE name LIKE '%\\_log.txt' AND NOT name LIKE '%h\\_%'",
        dir_path.display()
    );
    println!("Query: {}", query);

    execute_sql_query(&query);
}

/// Execute a SQL query and print the results
fn execute_sql_query(query: &str) {
    match parse_sql(query) {
        Ok(parsed_query) => match execute_query(&parsed_query) {
            Ok(results) => {
                println!("Operation affected {} files:", results.len());
                for file in &results {
                    println!("  {} → {}", file.name, file.path.display());
                }
            }
            Err(err) => {
                eprintln!("Error executing query: {}", err);
            }
        },
        Err(err) => {
            eprintln!("Error parsing SQL query: {}", err);
        }
    }
}

/// List all files in the directory after the operations
fn list_all_files(dir_path: &Path) {
    let query = format!("WITH RECURSIVE SELECT * FROM {}", dir_path.display());

    match parse_sql(&query) {
        Ok(parsed_query) => match execute_query(&parsed_query) {
            Ok(results) => {
                println!("Files in directory after renaming operations:");
                for file in &results {
                    println!("  {}", file.path.display());
                }
            }
            Err(err) => {
                eprintln!("Error executing query: {}", err);
            }
        },
        Err(err) => {
            eprintln!("Error parsing SQL query: {}", err);
        }
    }
}

