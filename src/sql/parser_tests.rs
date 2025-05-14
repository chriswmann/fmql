#[cfg(test)]
use crate::sql::ast::{ComparisonOperator, FileAttribute, FileCondition, FileQuery, FileValue};
use crate::sql::parser::{parse_sql, ParserError};

#[test]
fn test_parse_select_all() {
    let sql = "SELECT * FROM ~/Documents";
    let query = parse_sql(sql).unwrap();
    
    match query {
        FileQuery::Select { path, recursive, attributes, condition } => {
            assert!(path.ends_with("Documents"));
            assert!(!recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));
            assert!(condition.is_none());
        },
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_select_with_condition() {
    let sql = "SELECT * FROM ~/Documents WHERE extension = '.txt'";
    let query = parse_sql(sql).unwrap();
    
    match query {
        FileQuery::Select { path, recursive, attributes, condition } => {
            assert!(path.ends_with("Documents"));
            assert!(!recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));
            
            match condition {
                Some(FileCondition::Compare { attribute, operator, value }) => {
                    assert!(matches!(attribute, FileAttribute::Extension));
                    assert!(matches!(operator, ComparisonOperator::Eq));
                    assert!(matches!(value, FileValue::String(s) if s == ".txt"));
                },
                _ => panic!("Expected Compare condition"),
            }
        },
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_select_with_complex_condition() {
    let sql = "SELECT * FROM ~/Documents WHERE extension = '.txt' AND size > 1000";
    let query = parse_sql(sql).unwrap();
    
    match query {
        FileQuery::Select { path, recursive, attributes, condition } => {
            assert!(path.ends_with("Documents"));
            assert!(!recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));
            
            match condition {
                Some(FileCondition::And(left, right)) => {
                    match (*left, *right) {
                        (
                            FileCondition::Compare { 
                                attribute: attr1, 
                                operator: op1, 
                                value: val1 
                            },
                            FileCondition::Compare { 
                                attribute: attr2, 
                                operator: op2, 
                                value: val2 
                            }
                        ) => {
                            assert!(matches!(attr1, FileAttribute::Extension));
                            assert!(matches!(op1, ComparisonOperator::Eq));
                            assert!(matches!(val1, FileValue::String(s) if s == ".txt"));
                            
                            assert!(matches!(attr2, FileAttribute::Size));
                            assert!(matches!(op2, ComparisonOperator::Gt));
                            assert!(matches!(val2, FileValue::Number(n) if n == 1000.0));
                        },
                        _ => panic!("Expected two Compare conditions"),
                    }
                },
                _ => panic!("Expected AND condition"),
            }
        },
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_select_with_between() {
    let sql = "SELECT * FROM ~/Documents WHERE modified BETWEEN '2025-01-01' AND '2025-03-31'";
    let query = parse_sql(sql).unwrap();
    
    match query {
        FileQuery::Select { path, recursive, attributes, condition } => {
            assert!(path.ends_with("Documents"));
            assert!(!recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));
            
            match condition {
                Some(FileCondition::Between { attribute, lower, upper }) => {
                    assert!(matches!(attribute, FileAttribute::Modified));
                    assert!(matches!(lower, FileValue::String(s) if s == "2025-01-01"));
                    assert!(matches!(upper, FileValue::String(s) if s == "2025-03-31"));
                },
                _ => panic!("Expected BETWEEN condition"),
            }
        },
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_select_with_like() {
    let sql = "SELECT * FROM ~/Projects WHERE name LIKE '%config%'";
    let query = parse_sql(sql).unwrap();
    
    match query {
        FileQuery::Select { path, recursive, attributes, condition } => {
            assert!(path.ends_with("Projects"));
            assert!(!recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));
            
            match condition {
                Some(FileCondition::Like { attribute, pattern, case_sensitive }) => {
                    assert!(matches!(attribute, FileAttribute::Name));
                    assert_eq!(pattern, "%config%");
                    assert!(!case_sensitive);
                },
                _ => panic!("Expected LIKE condition"),
            }
        },
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_select_with_regexp() {
    let sql = "SELECT * FROM ~/logs WHERE REGEXP(name, '^server_[0-9]+\\.log$')";
    let query = parse_sql(sql).unwrap();
    
    match query {
        FileQuery::Select { path, recursive, attributes, condition } => {
            assert!(path.ends_with("logs"));
            assert!(!recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));
            
            match condition {
                Some(FileCondition::Regexp { attribute, pattern }) => {
                    assert!(matches!(attribute, FileAttribute::Name));
                    assert_eq!(pattern, "^server_[0-9]+\\.log$");
                },
                _ => panic!("Expected REGEXP condition"),
            }
        },
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_select_with_recursive() {
    let sql = "WITH RECURSIVE SELECT * FROM ~/Projects WHERE name LIKE '%config%'";
    let query = parse_sql(sql).unwrap();
    
    match query {
        FileQuery::Select { path, recursive, attributes, condition } => {
            assert!(path.ends_with("Projects"));
            assert!(recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));
            
            match condition {
                Some(FileCondition::Like { attribute, pattern, case_sensitive }) => {
                    assert!(matches!(attribute, FileAttribute::Name));
                    assert_eq!(pattern, "%config%");
                    assert!(!case_sensitive);
                },
                _ => panic!("Expected LIKE condition"),
            }
        },
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_update() {
    let sql = "UPDATE ~/executables SET permissions = '755' WHERE extension = '.bin'";
    let query = parse_sql(sql).unwrap();
    
    match query {
        FileQuery::Update { path, updates, condition } => {
            assert!(path.ends_with("executables"));
            assert_eq!(updates.len(), 1);
            
            let update = &updates[0];
            assert!(matches!(update.attribute, FileAttribute::Permissions));
            assert_eq!(update.value, "755");
            
            match condition {
                Some(FileCondition::Compare { attribute, operator, value }) => {
                    assert!(matches!(attribute, FileAttribute::Extension));
                    assert!(matches!(operator, ComparisonOperator::Eq));
                    assert!(matches!(value, FileValue::String(s) if s == ".bin"));
                },
                _ => panic!("Expected Compare condition"),
            }
        },
        _ => panic!("Expected UPDATE query"),
    }
}

#[test]
fn test_parse_update_multiple_attributes() {
    let sql = "UPDATE ~/executables SET owner = 'admin', permissions = '755' WHERE extension = '.bin'";
    let query = parse_sql(sql).unwrap();
    
    match query {
        FileQuery::Update { path, updates, condition } => {
            assert!(path.ends_with("executables"));
            assert_eq!(updates.len(), 2);
            
            assert!(matches!(updates[0].attribute, FileAttribute::Owner));
            assert_eq!(updates[0].value, "admin");
            
            assert!(matches!(updates[1].attribute, FileAttribute::Permissions));
            assert_eq!(updates[1].value, "755");
            
            match condition {
                Some(FileCondition::Compare { attribute, operator, value }) => {
                    assert!(matches!(attribute, FileAttribute::Extension));
                    assert!(matches!(operator, ComparisonOperator::Eq));
                    assert!(matches!(value, FileValue::String(s) if s == ".bin"));
                },
                _ => panic!("Expected Compare condition"),
            }
        },
        _ => panic!("Expected UPDATE query"),
    }
}

#[test]
fn test_parse_invalid_sql() {
    let sql = "INSERT INTO ~/Documents VALUES ('file.txt')";
    let result = parse_sql(sql);
    
    assert!(result.is_err());
    match result {
        Err(ParserError::UnsupportedStatement(_)) => {},
        _ => panic!("Expected UnsupportedStatement error"),
    }
}
