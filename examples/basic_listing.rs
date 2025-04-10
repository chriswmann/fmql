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
    
    // Create some test files with different sizes and extensions
    create_file(&dir, "document.txt", "This is a text document.");
    create_file(&dir, "image.jpg", &vec![0; 2000].into_iter().map(|_| b'X').collect::<Vec<u8>>());
    create_file(&dir, "config.ini", "[settings]\nmode=dark\nsize=large");
    create_file(&dir, "script.sh", "#!/bin/bash\necho 'Hello, world!'");
    create_file(&dir, "data.csv", "id,name,age\n1,Alice,28\n2,Bob,34\n3,Charlie,45");
    
    // Create a subdirectory with more files
    let subdir_path = dir.path().join("subdir");
    fs::create_dir(&subdir_path).expect("Failed to create subdirectory");
    create_file(&dir, "subdir/nested.txt", "This is a nested file");
    create_file(&dir, "subdir/config.xml", "<config><setting>value</setting></config>");
    
    // Make script executable on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script_path = dir.path().join("script.sh");
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
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
    
    println!("\n=== Basic File Listing ===");
    basic_listing(&dir_path);
    
    println!("\n=== Long Format Listing ===");
    long_format_listing(&dir_path);
    
    println!("\n=== Recursive Listing ===");
    recursive_listing(&dir_path);
    
    println!("\n=== Sorted Listing ===");
    sorted_listing(&dir_path);
    
    // The temporary directory will be automatically cleaned up when temp_dir goes out of scope
    println!("\nTemporary directory will be cleaned up automatically");
}

/// Basic file listing example
fn basic_listing(dir_path: &PathBuf) {
    let args = Args {
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
    
    match list_directory(&args) {
        Ok(files) => {
            println!("Found {} files:", files.len());
            display_file_list(&files, &args);
        },
        Err(err) => {
            eprintln!("Error listing files: {}", err);
        }
    }
}

/// Long format listing example
fn long_format_listing(dir_path: &PathBuf) {
    let args = Args {
        path: dir_path.clone(),
        long_view: true,
        show_total: true,
        recursive: false,
        show_hidden: false,
        group_by: GroupByOption::None,
        name_pattern: None,
        sort_by: SortOption::Name,
        output_format: OutputFormat::Text,
    };
    
    match list_directory(&args) {
        Ok(files) => {
            println!("Found {} files (detailed view):", files.len());
            display_file_list(&files, &args);
        },
        Err(err) => {
            eprintln!("Error listing files: {}", err);
        }
    }
}

/// Recursive listing example
fn recursive_listing(dir_path: &PathBuf) {
    let args = Args {
        path: dir_path.clone(),
        long_view: false,
        show_total: false,
        recursive: true,
        show_hidden: false,
        group_by: GroupByOption::None,
        name_pattern: None,
        sort_by: SortOption::Name,
        output_format: OutputFormat::Text,
    };
    
    match list_directory(&args) {
        Ok(files) => {
            println!("Found {} files (including subdirectories):", files.len());
            display_file_list(&files, &args);
        },
        Err(err) => {
            eprintln!("Error listing files: {}", err);
        }
    }
}

/// Sorted listing example
fn sorted_listing(dir_path: &PathBuf) {
    // Try different sorting options
    let sort_options = [
        (SortOption::Name, "by name"),
        (SortOption::Size, "by size"),
        (SortOption::Modified, "by modification time"),
        (SortOption::Type, "by type"),
    ];
    
    for (sort_option, description) in sort_options.iter() {
        let args = Args {
            path: dir_path.clone(),
            long_view: true,
            show_total: false,
            recursive: false,
            show_hidden: false,
            group_by: GroupByOption::None,
            name_pattern: None,
            sort_by: sort_option.clone(),
            output_format: OutputFormat::Text,
        };
        
        match list_directory(&args) {
            Ok(files) => {
                println!("\nListing files sorted {}:", description);
                display_file_list(&files, &args);
            },
            Err(err) => {
                eprintln!("Error listing files: {}", err);
            }
        }
    }
} 