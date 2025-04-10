#[cfg(test)]
mod tests {
    use crate::sql::ast::{FileAttribute, FileAttributeUpdate, FileCondition, FileQuery, FileValue, ComparisonOperator};
    use crate::sql::executor::{execute_query, ExecutorError, FileResult};
    use crate::sql::parser::parse_sql;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::{tempdir, TempDir};
    use chrono::Local;
    use std::io::{Result as IoResult};
    use std::path::Path;
    
    fn setup_test_directory() -> TempDir {
        let dir = tempdir().unwrap();
        
        // Create some test files
        let _ = create_test_file(dir.path(), "file1.txt", "Hello, world!");
        let _ = create_test_file(dir.path(), "file2.txt", "This is a test.");
        let _ = create_test_file(dir.path(), "config.ini", "[section]\nkey=value");
        let _ = create_test_file(dir.path(), "script.sh", "#!/bin/bash\necho 'Hello'");
        
        // Create a subdirectory
        let subdir_path = dir.path().join("subdir");
        fs::create_dir(&subdir_path).unwrap();
        let _ = create_test_file(dir.path(), "subdir/file3.txt", "Nested file.");
        let _ = create_test_file(dir.path(), "subdir/config.xml", "<config></config>");
        
        // Make script.sh executable
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
    
    fn create_test_file(directory: &Path, filename: &str, content: &str) -> IoResult<PathBuf> {
        let path = directory.join(filename);
        let mut file = File::create(&path)?;
        file.write_all(content.as_bytes())?;
        Ok(path)
    }
    
    #[test]
    fn test_execute_select_all() {
        let dir = setup_test_directory();
        let dir_path = dir.path().to_string_lossy().to_string();
        
        let query = FileQuery::Select {
            path: dir.path().to_path_buf(),
            recursive: false,
            attributes: vec![FileAttribute::All],
            condition: None,
        };
        
        let results = execute_query(&query).unwrap();
        
        // There should be 6 files (including the directory itself)
        assert_eq!(results.len(), 6);
        
        // Verify some basic properties
        let files: Vec<_> = results.iter()
            .filter(|f| !f.is_directory)
            .map(|f| f.name.clone())
            .collect();
        
        assert!(files.contains(&"file1.txt".to_string()));
        assert!(files.contains(&"file2.txt".to_string()));
        assert!(files.contains(&"config.ini".to_string()));
        assert!(files.contains(&"script.sh".to_string()));
    }
    
    #[test]
    fn test_execute_select_recursive() {
        let dir = setup_test_directory();
        
        let query = FileQuery::Select {
            path: dir.path().to_path_buf(),
            recursive: true,
            attributes: vec![FileAttribute::All],
            condition: None,
        };
        
        let results = execute_query(&query).unwrap();
        
        // There should be 8 files (including the directories)
        assert_eq!(results.len(), 8);
        
        let all_files: Vec<_> = results.iter()
            .map(|f| f.path.to_string_lossy().to_string())
            .collect();
        
        // Check for the presence of subdirectory files
        assert!(all_files.iter().any(|p| p.contains("subdir/file3.txt")));
        assert!(all_files.iter().any(|p| p.contains("subdir/config.xml")));
    }
    
    #[test]
    fn test_execute_select_with_extension_filter() {
        let dir = setup_test_directory();
        
        let condition = FileCondition::Compare {
            attribute: FileAttribute::Extension,
            operator: ComparisonOperator::Eq,
            value: FileValue::String("txt".to_string()),
        };
        
        let query = FileQuery::Select {
            path: dir.path().to_path_buf(),
            recursive: true,
            attributes: vec![FileAttribute::All],
            condition: Some(condition),
        };
        
        let results = execute_query(&query).unwrap();
        
        // There should be 3 .txt files
        assert_eq!(results.len(), 3);
        
        // Verify they all have .txt extension
        for file in &results {
            assert_eq!(file.extension.as_ref().unwrap(), "txt");
        }
    }
    
    #[test]
    fn test_execute_select_with_name_like() {
        let dir = setup_test_directory();
        
        let condition = FileCondition::Like {
            attribute: FileAttribute::Name,
            pattern: "%config%".to_string(),
            case_sensitive: false,
        };
        
        let query = FileQuery::Select {
            path: dir.path().to_path_buf(),
            recursive: true,
            attributes: vec![FileAttribute::All],
            condition: Some(condition),
        };
        
        let results = execute_query(&query).unwrap();
        
        // There should be 2 files with "config" in the name
        assert_eq!(results.len(), 2);
        
        let names: Vec<_> = results.iter().map(|f| f.name.clone()).collect();
        assert!(names.contains(&"config.ini".to_string()));
        assert!(names.contains(&"config.xml".to_string()));
    }
    
    #[test]
    fn test_execute_select_with_size_comparison() {
        let dir = setup_test_directory();
        
        let condition = FileCondition::Compare {
            attribute: FileAttribute::Size,
            operator: ComparisonOperator::Gt,
            value: FileValue::Number(10.0),
        };
        
        let query = FileQuery::Select {
            path: dir.path().to_path_buf(),
            recursive: false,
            attributes: vec![FileAttribute::All],
            condition: Some(condition),
        };
        
        let results = execute_query(&query).unwrap();
        
        // Check that we only got files larger than 10 bytes
        for file in &results {
            assert!(file.size > 10);
        }
    }
    
    #[test]
    fn test_execute_select_with_complex_condition() {
        let dir = setup_test_directory();
        
        // Find .txt files with size > 5 bytes
        let txt_condition = FileCondition::Compare {
            attribute: FileAttribute::Extension,
            operator: ComparisonOperator::Eq,
            value: FileValue::String("txt".to_string()),
        };
        
        let size_condition = FileCondition::Compare {
            attribute: FileAttribute::Size,
            operator: ComparisonOperator::Gt,
            value: FileValue::Number(5.0),
        };
        
        let combined_condition = FileCondition::And(
            Box::new(txt_condition),
            Box::new(size_condition),
        );
        
        let query = FileQuery::Select {
            path: dir.path().to_path_buf(),
            recursive: true,
            attributes: vec![FileAttribute::All],
            condition: Some(combined_condition),
        };
        
        let results = execute_query(&query).unwrap();
        
        // Verify results
        for file in &results {
            assert_eq!(file.extension.as_ref().unwrap(), "txt");
            assert!(file.size > 5);
        }
    }
    
    #[test]
    fn test_execute_select_with_regexp() {
        let dir = setup_test_directory();
        
        let condition = FileCondition::Regexp {
            attribute: FileAttribute::Name,
            pattern: "^file[0-9]\\.txt$".to_string(),
        };
        
        let query = FileQuery::Select {
            path: dir.path().to_path_buf(),
            recursive: true,
            attributes: vec![FileAttribute::All],
            condition: Some(condition),
        };
        
        let results = execute_query(&query).unwrap();
        
        // There should be 3 files matching the pattern (file1.txt, file2.txt, and file3.txt)
        assert_eq!(results.len(), 3);
        
        let names: Vec<_> = results.iter().map(|f| f.name.clone()).collect();
        assert!(names.contains(&"file1.txt".to_string()));
        assert!(names.contains(&"file2.txt".to_string()));
        assert!(names.contains(&"file3.txt".to_string()));
    }
    
    #[test]
    #[cfg(unix)]
    fn test_execute_update_permissions() {
        let dir = setup_test_directory();
        
        let condition = FileCondition::Compare {
            attribute: FileAttribute::Extension,
            operator: ComparisonOperator::Eq,
            value: FileValue::String("txt".to_string()),
        };
        
        let updates = vec![
            FileAttributeUpdate {
                attribute: FileAttribute::Permissions,
                value: "644".to_string(),
            },
        ];
        
        let query = FileQuery::Update {
            path: dir.path().to_path_buf(),
            updates,
            condition: Some(condition),
        };
        
        let updated_files = execute_query(&query).unwrap();
        
        // Check that permissions were updated
        for file in &updated_files {
            assert_eq!(file.permissions & 0o777, 0o644);
        }
    }
} 