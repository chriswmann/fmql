use ls_rs::sql::{parse_sql, execute_query};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use tempfile::{tempdir, TempDir};

/// Creates sample files in a temporary directory for testing SQL queries
fn setup_test_directory() -> TempDir {
    let dir = tempdir().expect("Failed to create temp directory");
    println!("Created temporary directory at: {}", dir.path().display());
    
    // Create various files with different attributes for SQL query testing
    create_file(&dir, "document1.txt", "This is document 1", 0o644);
    create_file(&dir, "document2.txt", "This is document 2 with more content", 0o644);
    create_file(&dir, "document3.txt", "This is a larger document with even more content for testing", 0o644);
    create_file(&dir, "image1.jpg", &vec![0; 10000].into_iter().map(|_| b'X').collect::<Vec<u8>>(), 0o644);
    create_file(&dir, "image2.jpg", &vec![0; 20000].into_iter().map(|_| b'X').collect::<Vec<u8>>(), 0o644);
    create_file(&dir, "image3.png", &vec![0; 30000].into_iter().map(|_| b'X').collect::<Vec<u8>>(), 0o644);
    create_file(&dir, "script1.sh", "#!/bin/bash\necho 'Hello, World!'", 0o755);
    create_file(&dir, "script2.sh", "#!/bin/bash\nfor i in {1..5}; do\n  echo $i\ndone", 0o755);
    create_file(&dir, "config.ini", "[settings]\ntheme=dark\nmode=advanced", 0o644);
    create_file(&dir, "config.xml", "<config><theme>light</theme><mode>basic</mode></config>", 0o644);
    
    // Create subdirectories with more files
    let subdirs = ["documents", "images", "scripts", "configs"];
    for subdir in &subdirs {
        fs::create_dir(dir.path().join(subdir)).expect("Failed to create subdirectory");
    }
    
    // Add files to subdirectories
    create_file(&dir, "documents/report.txt", "Annual report content", 0o644);
    create_file(&dir, "documents/memo.txt", "Office memo", 0o644);
    create_file(&dir, "images/photo1.jpg", &vec![0; 40000].into_iter().map(|_| b'X').collect::<Vec<u8>>(), 0o644);
    create_file(&dir, "images/photo2.png", &vec![0; 50000].into_iter().map(|_| b'X').collect::<Vec<u8>>(), 0o644);
    create_file(&dir, "scripts/backup.sh", "#!/bin/bash\necho 'Backing up files...'", 0o755);
    create_file(&dir, "scripts/install.sh", "#!/bin/bash\necho 'Installing software...'", 0o755);
    create_file(&dir, "configs/app.config", "# App configuration\nversion=1.0\ndebug=true", 0o644);
    
    // Wait a bit to ensure file timestamps differ
    std::thread::sleep(std::time::Duration::from_millis(100));
    
    // Create some newer files to test timestamp queries
    create_file(&dir, "recent1.txt", "Recent file 1", 0o644);
    create_file(&dir, "recent2.log", "Log entry 1\nLog entry 2", 0o644);
    
    dir
}

/// Helper function to create a file with the given content and permissions
fn create_file(dir: &TempDir, path: &str, content: &(impl AsRef<[u8]> + ?Sized), mode: u32) {
    let full_path = dir.path().join(path);
    
    // Create parent directories if needed
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).expect("Failed to create parent directory");
    }
    
    let mut file = File::create(&full_path).expect("Failed to create file");
    file.write_all(content.as_ref()).expect("Failed to write content");
    
    // Set file permissions on Unix systems
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(mode);
        fs::set_permissions(&full_path, perms).expect("Failed to set permissions");
    }
    
    println!("Created file: {}", full_path.display());
}

fn main() {
    // Setup test directory with sample files
    let temp_dir = setup_test_directory();
    let dir_path = temp_dir.path().to_path_buf();
    
    println!("\n=== Basic SELECT Queries ===");
    basic_select_queries(&dir_path);
    
    println!("\n=== Filtering with WHERE Clause ===");
    filtering_queries(&dir_path);
    
    println!("\n=== Complex Conditions ===");
    complex_conditions(&dir_path);
    
    println!("\n=== Pattern Matching Queries ===");
    pattern_matching(&dir_path);
    
    println!("\n=== Recursive Queries ===");
    recursive_queries(&dir_path);
    
    #[cfg(unix)]
    {
        println!("\n=== Update Queries ===");
        update_queries(&dir_path);
    }
    
    // The temporary directory will be automatically cleaned up when temp_dir goes out of scope
    println!("\nTemporary directory will be cleaned up automatically");
}

/// Basic SELECT queries without filtering
fn basic_select_queries(dir_path: &PathBuf) {
    let query = format!("SELECT * FROM {}", dir_path.display());
    println!("Query: {}", query);
    
    match parse_sql(&query) {
        Ok(parsed_query) => {
            match execute_query(&parsed_query) {
                Ok(results) => {
                    println!("Found {} files in the root directory:", results.len());
                    for file in &results {
                        println!("  {} ({})", file.name, file.size);
                    }
                },
                Err(err) => {
                    eprintln!("Error executing query: {}", err);
                }
            }
        },
        Err(err) => {
            eprintln!("Error parsing SQL query: {}", err);
        }
    }
}

/// Queries with WHERE clause for filtering
fn filtering_queries(dir_path: &PathBuf) {
    let queries = [
        // Query for text files
        format!("SELECT * FROM {} WHERE extension = 'txt'", dir_path.display()),
        // Query for files larger than 10KB
        format!("SELECT * FROM {} WHERE size > 10000", dir_path.display()),
        // Query for executable scripts
        format!("SELECT * FROM {} WHERE name LIKE '%.sh'", dir_path.display()),
    ];
    
    for query in &queries {
        println!("\nQuery: {}", query);
        
        match parse_sql(query) {
            Ok(parsed_query) => {
                match execute_query(&parsed_query) {
                    Ok(results) => {
                        println!("Found {} matching files:", results.len());
                        for file in &results {
                            println!("  {} ({})", file.name, file.size);
                        }
                    },
                    Err(err) => {
                        eprintln!("Error executing query: {}", err);
                    }
                }
            },
            Err(err) => {
                eprintln!("Error parsing SQL query: {}", err);
            }
        }
    }
}

/// Queries with complex conditions using AND/OR
fn complex_conditions(dir_path: &PathBuf) {
    let queries = [
        // Query for text files larger than 20 bytes
        format!("SELECT * FROM {} WHERE extension = 'txt' AND size > 20", dir_path.display()),
        // Query for JPG or PNG files
        format!("SELECT * FROM {} WHERE extension = 'jpg' OR extension = 'png'", dir_path.display()),
        // Query for config files with specific content
        format!("SELECT * FROM {} WHERE name LIKE '%config%' AND size < 500", dir_path.display()),
    ];
    
    for query in &queries {
        println!("\nQuery: {}", query);
        
        match parse_sql(query) {
            Ok(parsed_query) => {
                match execute_query(&parsed_query) {
                    Ok(results) => {
                        println!("Found {} matching files:", results.len());
                        for file in &results {
                            println!("  {} ({})", file.name, file.size);
                        }
                    },
                    Err(err) => {
                        eprintln!("Error executing query: {}", err);
                    }
                }
            },
            Err(err) => {
                eprintln!("Error parsing SQL query: {}", err);
            }
        }
    }
}

/// Queries with pattern matching (LIKE and REGEXP)
fn pattern_matching(dir_path: &PathBuf) {
    let queries = [
        // LIKE pattern matching for filenames starting with "doc"
        format!("SELECT * FROM {} WHERE name LIKE 'doc%'", dir_path.display()),
        // LIKE pattern matching for any file with "config" in the name
        format!("SELECT * FROM {} WHERE name LIKE '%config%'", dir_path.display()),
        // REGEXP pattern matching for script files
        format!("SELECT * FROM {} WHERE REGEXP(name, '^script[0-9]+\\.sh$')", dir_path.display()),
    ];
    
    for query in &queries {
        println!("\nQuery: {}", query);
        
        match parse_sql(query) {
            Ok(parsed_query) => {
                match execute_query(&parsed_query) {
                    Ok(results) => {
                        println!("Found {} matching files:", results.len());
                        for file in &results {
                            println!("  {} ({})", file.name, file.size);
                        }
                    },
                    Err(err) => {
                        eprintln!("Error executing query: {}", err);
                    }
                }
            },
            Err(err) => {
                eprintln!("Error parsing SQL query: {}", err);
            }
        }
    }
}

/// Recursive queries to search in subdirectories
fn recursive_queries(dir_path: &PathBuf) {
    let query = format!("WITH RECURSIVE SELECT * FROM {} WHERE extension = 'txt'", dir_path.display());
    println!("Query: {}", query);
    
    match parse_sql(&query) {
        Ok(parsed_query) => {
            match execute_query(&parsed_query) {
                Ok(results) => {
                    println!("Found {} text files recursively:", results.len());
                    for file in &results {
                        println!("  {} ({})", file.path.display(), file.size);
                    }
                },
                Err(err) => {
                    eprintln!("Error executing query: {}", err);
                }
            }
        },
        Err(err) => {
            eprintln!("Error parsing SQL query: {}", err);
        }
    }
}

/// Update queries for changing file attributes (Unix only)
#[cfg(unix)]
fn update_queries(dir_path: &PathBuf) {
    // Query to make all shell scripts executable
    let query = format!("UPDATE {} SET permissions = '755' WHERE extension = 'sh'", dir_path.display());
    println!("Query: {}", query);
    
    match parse_sql(&query) {
        Ok(parsed_query) => {
            match execute_query(&parsed_query) {
                Ok(results) => {
                    println!("Updated {} files:", results.len());
                    for file in &results {
                        println!("  {} (permissions: {:o})", file.path.display(), file.permissions);
                    }
                },
                Err(err) => {
                    eprintln!("Error executing query: {}", err);
                }
            }
        },
        Err(err) => {
            eprintln!("Error parsing SQL query: {}", err);
        }
    }
} 