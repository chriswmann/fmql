use ls_rs::cli::{Args, GroupByOption, SortOption, OutputFormat};
use ls_rs::file::list_directory;
use ls_rs::display::display_file_list;
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use tempfile::{tempdir, TempDir};

/// Creates sample files in a temporary directory for testing
fn setup_test_directory() -> TempDir {
    let dir = tempdir().expect("Failed to create temp directory");
    println!("Created temporary directory at: {}", dir.path().display());
    
    // Create files with different extensions
    create_file(&dir, "doc1.txt", "Document 1 content");
    create_file(&dir, "doc2.txt", "Document 2 content");
    create_file(&dir, "image1.jpg", &vec![0; 1500].into_iter().map(|_| b'X').collect::<Vec<u8>>());
    create_file(&dir, "image2.jpg", &vec![0; 2500].into_iter().map(|_| b'X').collect::<Vec<u8>>());
    create_file(&dir, "image3.png", &vec![0; 3500].into_iter().map(|_| b'X').collect::<Vec<u8>>());
    create_file(&dir, "config.ini", "[settings]\nmode=dark");
    create_file(&dir, "script1.sh", "#!/bin/bash\necho 'Hello'");
    create_file(&dir, "script2.sh", "#!/bin/bash\necho 'World'");
    
    // Create files with hidden files
    create_file(&dir, ".hidden1", "This is a hidden file");
    create_file(&dir, ".hidden2", "This is another hidden file");
    
    // Create subdirectories with more files
    let subdir_paths = [
        dir.path().join("documents"),
        dir.path().join("images"),
        dir.path().join("code")
    ];
    
    for path in &subdir_paths {
        fs::create_dir(path).expect("Failed to create subdirectory");
    }
    
    create_file(&dir, "documents/report.txt", "Report content");
    create_file(&dir, "documents/letter.doc", "Letter content");
    create_file(&dir, "images/vacation.jpg", &vec![0; 4000].into_iter().map(|_| b'X').collect::<Vec<u8>>());
    create_file(&dir, "code/main.rs", "fn main() { println!(\"Hello, world!\"); }");
    create_file(&dir, "code/utils.rs", "pub fn add(a: i32, b: i32) -> i32 { a + b }");
    
    // Make scripts executable on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for script in ["script1.sh", "script2.sh"] {
            let script_path = dir.path().join(script);
            let mut perms = fs::metadata(&script_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&script_path, perms).unwrap();
        }
    }
    
    dir
}

/// Helper function to create a file with the given content
fn create_file(dir: &TempDir, path: &str, content: &(impl AsRef<[u8]> + ?Sized)) {
    let full_path = dir.path().join(path);
    
    // Create parent directories if needed
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).expect("Failed to create parent directory");
    }
    
    let mut file = File::create(&full_path).expect("Failed to create file");
    file.write_all(content.as_ref()).expect("Failed to write content");
    
    println!("Created file: {}", full_path.display());
}

fn main() {
    // Setup test directory with sample files
    let temp_dir = setup_test_directory();
    let dir_path = temp_dir.path().to_path_buf();
    
    println!("\n=== Filtering By Name Pattern ===");
    filter_by_pattern(&dir_path);
    
    println!("\n=== Including Hidden Files ===");
    show_hidden_files(&dir_path);
    
    println!("\n=== Grouping By Extension ===");
    group_by_extension(&dir_path);
    
    println!("\n=== Grouping By File Type ===");
    group_by_type(&dir_path);
    
    println!("\n=== Grouping By File Size ===");
    group_by_size(&dir_path);
    
    println!("\n=== Different Output Formats ===");
    output_formats(&dir_path);
    
    // The temporary directory will be automatically cleaned up when temp_dir goes out of scope
    println!("\nTemporary directory will be cleaned up automatically");
}

/// Example of filtering files by name pattern
fn filter_by_pattern(dir_path: &PathBuf) {
    // Array of patterns to demonstrate
    let patterns = [
        "*.txt",   // Text files
        "image*",  // Files starting with 'image'
        "*.sh",    // Shell scripts
        "*.rs"     // Rust files
    ];
    
    for pattern in patterns.iter() {
        let args = Args {
            path: dir_path.clone(),
            long_view: false,
            show_total: false,
            recursive: true,
            show_hidden: false,
            group_by: GroupByOption::None,
            name_pattern: Some(pattern.to_string()),
            sort_by: SortOption::Name,
            output_format: OutputFormat::Text,
        };
        
        match list_directory(&args) {
            Ok(files) => {
                println!("\nFiles matching pattern '{}':", pattern);
                display_file_list(&files, &args);
            },
            Err(err) => {
                eprintln!("Error listing files with pattern '{}': {}", pattern, err);
            }
        }
    }
}

/// Example of showing hidden files
fn show_hidden_files(dir_path: &PathBuf) {
    let args_without_hidden = Args {
        path: dir_path.clone(),
        long_view: false,
        show_total: false,
        recursive: false,
        show_hidden: false,
        group_by: GroupByOption::None,
        name_pattern: None,
        sort_by: SortOption::Name,
        output_format: OutputFormat::Text,
    };
    
    let args_with_hidden = Args {
        path: dir_path.clone(),
        long_view: false,
        show_total: false,
        recursive: false,
        show_hidden: true,
        group_by: GroupByOption::None,
        name_pattern: None,
        sort_by: SortOption::Name,
        output_format: OutputFormat::Text,
    };
    
    match list_directory(&args_without_hidden) {
        Ok(files) => {
            println!("Files (without hidden):");
            display_file_list(&files, &args_without_hidden);
        },
        Err(err) => {
            eprintln!("Error listing files: {}", err);
        }
    }
    
    match list_directory(&args_with_hidden) {
        Ok(files) => {
            println!("\nFiles (including hidden):");
            display_file_list(&files, &args_with_hidden);
        },
        Err(err) => {
            eprintln!("Error listing files: {}", err);
        }
    }
}

/// Example of grouping files by extension
fn group_by_extension(dir_path: &PathBuf) {
    let args = Args {
        path: dir_path.clone(),
        long_view: false,
        show_total: false,
        recursive: true,
        show_hidden: false,
        group_by: GroupByOption::Extension,
        name_pattern: None,
        sort_by: SortOption::Name,
        output_format: OutputFormat::Text,
    };
    
    match list_directory(&args) {
        Ok(files) => {
            println!("Files grouped by extension:");
            display_file_list(&files, &args);
        },
        Err(err) => {
            eprintln!("Error listing files: {}", err);
        }
    }
}

/// Example of grouping files by type (directory, file, etc.)
fn group_by_type(dir_path: &PathBuf) {
    let args = Args {
        path: dir_path.clone(),
        long_view: false,
        show_total: false,
        recursive: true,
        show_hidden: false,
        group_by: GroupByOption::Folder,
        name_pattern: None,
        sort_by: SortOption::Name,
        output_format: OutputFormat::Text,
    };
    
    match list_directory(&args) {
        Ok(files) => {
            println!("Files grouped by type:");
            display_file_list(&files, &args);
        },
        Err(err) => {
            eprintln!("Error listing files: {}", err);
        }
    }
}

/// Example of grouping files by size
fn group_by_size(dir_path: &PathBuf) {
    let args = Args {
        path: dir_path.clone(),
        long_view: false,
        show_total: false,
        recursive: true,
        show_hidden: false,
        group_by: GroupByOption::None,
        name_pattern: None,
        sort_by: SortOption::Size,
        output_format: OutputFormat::Text,
    };
    
    match list_directory(&args) {
        Ok(files) => {
            println!("Files grouped by size:");
            display_file_list(&files, &args);
        },
        Err(err) => {
            eprintln!("Error listing files: {}", err);
        }
    }
}

/// Example of different output formats
fn output_formats(dir_path: &PathBuf) {
    let formats = [
        (OutputFormat::Text, "Plain Text"),
        (OutputFormat::Text, "Text Format"),
        (OutputFormat::Text, "Text Format"),
    ];
    
    for (format, description) in formats.iter() {
        let args = Args {
            path: dir_path.clone(),
            long_view: true,
            show_total: false,
            recursive: false,
            show_hidden: false,
            group_by: GroupByOption::None,
            name_pattern: None,
            sort_by: SortOption::Name,
            output_format: format.clone(),
        };
        
        match list_directory(&args) {
            Ok(files) => {
                println!("\nListing files in {} format:", description);
                display_file_list(&files, &args);
            },
            Err(err) => {
                eprintln!("Error listing files: {}", err);
            }
        }
    }
} 