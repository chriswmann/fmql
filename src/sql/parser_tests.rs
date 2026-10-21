#[cfg(test)]
use crate::sql::ast::{
    ComparisonOperator, FileAttribute, FileCondition, FileQuery, FileValue, NameExpr, UpdateValue,
};
use crate::sql::parser::{ParserError, parse_sql, parse_sql_with_home};

#[test]
fn test_parse_select_all() {
    let sql = "SELECT * FROM ~/Documents";
    let query = parse_sql(sql).unwrap();

    match query {
        FileQuery::Select {
            path,
            recursive,
            attributes,
            condition,
            ..
        } => {
            assert!(path.ends_with("Documents"));
            assert!(!recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));
            assert!(condition.is_none());
        }
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_select_with_condition() {
    let sql = "SELECT * FROM ~/Documents WHERE extension = '.txt'";
    let query = parse_sql(sql).unwrap();

    match query {
        FileQuery::Select {
            path,
            recursive,
            attributes,
            condition,
            ..
        } => {
            assert!(path.ends_with("Documents"));
            assert!(!recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));

            match condition {
                Some(FileCondition::Compare {
                    attribute,
                    operator,
                    value,
                }) => {
                    assert!(matches!(attribute, FileAttribute::Extension));
                    assert!(matches!(operator, ComparisonOperator::Eq));
                    assert!(matches!(value, FileValue::String(s) if s == "txt"));
                }
                _ => panic!("Expected Compare condition"),
            }
        }
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_select_with_complex_condition() {
    let sql = "SELECT * FROM ~/Documents WHERE extension = '.txt' AND size > 1000";
    let query = parse_sql(sql).unwrap();

    match query {
        FileQuery::Select {
            path,
            recursive,
            attributes,
            condition,
            ..
        } => {
            assert!(path.ends_with("Documents"));
            assert!(!recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));

            match condition {
                Some(FileCondition::And(left, right)) => match (*left, *right) {
                    (
                        FileCondition::Compare {
                            attribute: attr1,
                            operator: op1,
                            value: val1,
                        },
                        FileCondition::Compare {
                            attribute: attr2,
                            operator: op2,
                            value: val2,
                        },
                    ) => {
                        assert!(matches!(attr1, FileAttribute::Extension));
                        assert!(matches!(op1, ComparisonOperator::Eq));
                        assert!(matches!(val1, FileValue::String(s) if s == "txt"));

                        assert!(matches!(attr2, FileAttribute::Size));
                        assert!(matches!(op2, ComparisonOperator::Gt));
                        assert!(
                            matches!(val2, FileValue::Number(n) if (n - 1000.0).abs() < f64::EPSILON)
                        );
                    }
                    _ => panic!("Expected two Compare conditions"),
                },
                _ => panic!("Expected AND condition"),
            }
        }
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_select_with_between() {
    let sql = "SELECT * FROM ~/Documents WHERE modified BETWEEN '2025-01-01' AND '2025-03-31'";
    let query = parse_sql(sql).unwrap();

    match query {
        FileQuery::Select {
            path,
            recursive,
            attributes,
            condition,
            ..
        } => {
            assert!(path.ends_with("Documents"));
            assert!(!recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));

            match condition {
                Some(FileCondition::Between {
                    attribute,
                    lower,
                    upper,
                }) => {
                    assert!(matches!(attribute, FileAttribute::Modified));
                    assert!(matches!(lower, FileValue::String(s) if s == "2025-01-01"));
                    assert!(matches!(upper, FileValue::String(s) if s == "2025-03-31"));
                }
                _ => panic!("Expected BETWEEN condition"),
            }
        }
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_select_with_like() {
    let sql = "SELECT * FROM ~/Projects WHERE name LIKE '%config%'";
    let query = parse_sql(sql).unwrap();

    match query {
        FileQuery::Select {
            path,
            recursive,
            attributes,
            condition,
            ..
        } => {
            assert!(path.ends_with("Projects"));
            assert!(!recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));

            match condition {
                Some(FileCondition::Like {
                    attribute,
                    pattern,
                    case_sensitive,
                }) => {
                    assert!(matches!(attribute, FileAttribute::Name));
                    assert_eq!(pattern, "%config%");
                    assert!(!case_sensitive);
                }
                _ => panic!("Expected LIKE condition"),
            }
        }
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_select_with_like_pattern_variations() {
    let test_cases = vec![
        ("SELECT * FROM ~/Downloads WHERE name LIKE '%meme'", "%meme"),
        ("SELECT * FROM ~/Documents WHERE name LIKE 'meme%'", "meme%"),
        (
            "SELECT * FROM ~/Pictures WHERE name LIKE '%meme%'",
            "%meme%",
        ),
        ("SELECT * FROM ~/Videos WHERE name LIKE 'm_me'", "m_me"),
        ("SELECT * FROM ~/Music WHERE name LIKE '_e%e_'", "_e%e_"),
    ];

    for (sql, expected_pattern) in test_cases {
        let query = parse_sql(sql).unwrap();
        match query {
            FileQuery::Select { condition, .. } => match condition {
                Some(FileCondition::Like {
                    attribute,
                    pattern,
                    case_sensitive,
                }) => {
                    assert!(matches!(attribute, FileAttribute::Name));
                    assert_eq!(pattern, expected_pattern);
                    assert!(
                        !case_sensitive,
                        "Expected case_sensitive to be false for: {sql}"
                    );
                }
                _ => panic!("Expected LIKE condition for SQL: {sql}"),
            },
            _ => panic!("Expected SELECT query for SQL: {sql}"),
        }
    }
}

#[test]
fn test_parse_select_with_regexp() {
    let sql = "SELECT * FROM ~/logs WHERE REGEXP(name, '^server_[0-9]+\\.log$')";
    let query = parse_sql(sql).unwrap();

    match query {
        FileQuery::Select {
            path,
            recursive,
            attributes,
            condition,
            ..
        } => {
            assert!(path.ends_with("logs"));
            assert!(!recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));

            match condition {
                Some(FileCondition::Regexp { attribute, pattern }) => {
                    assert!(matches!(attribute, FileAttribute::Name));
                    assert_eq!(pattern, "^server_[0-9]+\\.log$");
                }
                _ => panic!("Expected REGEXP condition"),
            }
        }
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_select_with_recursive() {
    let sql = "WITH RECURSIVE SELECT * FROM ~/Projects WHERE name LIKE '%config%'";
    let query = parse_sql(sql).unwrap();

    match query {
        FileQuery::Select {
            path,
            recursive,
            attributes,
            condition,
            ..
        } => {
            assert!(path.ends_with("Projects"));
            assert!(recursive);
            assert_eq!(attributes.len(), 1);
            assert!(matches!(attributes[0], FileAttribute::All));

            match condition {
                Some(FileCondition::Like {
                    attribute,
                    pattern,
                    case_sensitive,
                }) => {
                    assert!(matches!(attribute, FileAttribute::Name));
                    assert_eq!(pattern, "%config%");
                    assert!(!case_sensitive);
                }
                _ => panic!("Expected LIKE condition"),
            }
        }
        _ => panic!("Expected SELECT query"),
    }
}

#[test]
fn test_parse_update() {
    let sql = "UPDATE ~/executables SET permissions = '755' WHERE extension = '.bin'";
    let query = parse_sql(sql).unwrap();

    match query {
        FileQuery::Update {
            path,
            updates,
            condition,
            ..
        } => {
            assert!(path.ends_with("executables"));
            assert_eq!(updates.len(), 1);

            let update = &updates[0];
            assert!(matches!(update.attribute, FileAttribute::Permissions));
            assert_eq!(update.value, UpdateValue::Literal("755".to_string()));

            match condition {
                Some(FileCondition::Compare {
                    attribute,
                    operator,
                    value,
                }) => {
                    assert!(matches!(attribute, FileAttribute::Extension));
                    assert!(matches!(operator, ComparisonOperator::Eq));
                    assert!(matches!(value, FileValue::String(s) if s == "bin"));
                }
                _ => panic!("Expected Compare condition"),
            }
        }
        _ => panic!("Expected UPDATE query"),
    }
}

#[test]
fn test_parse_update_multiple_attributes() {
    let sql =
        "UPDATE ~/executables SET owner = 'admin', permissions = '755' WHERE extension = '.bin'";
    let FileQuery::Update { updates, .. } = parse_sql(sql).unwrap() else {
        panic!("Expected UPDATE query");
    };
    let assigned: Vec<_> = updates
        .iter()
        .map(|u| (u.attribute.clone(), u.value.clone()))
        .collect();
    assert_eq!(
        assigned,
        [
            (FileAttribute::Owner, UpdateValue::Literal("admin".into())),
            (
                FileAttribute::Permissions,
                UpdateValue::Literal("755".into())
            )
        ]
    );
    for sql in [
        "UPDATE . SET owner = 'a', owner = 'b'",
        "UPDATE . SET owner = ''",
        "UPDATE . SET owner = 1000",
        "UPDATE . SET owner = name",
        "UPDATE . SET size = 1",
        "UPDATE . SET owner = 'a',",
    ] {
        assert!(parse_sql(sql).is_err(), "accepted {sql}");
    }
    assert!(matches!(
        parse_sql("UPDATE . SET permissions = 600, PERMISSIONS = 700"),
        Err(ParserError::DuplicateAssignment(name)) if name == "permissions"
    ));
}

#[test]
fn test_parse_invalid_sql() {
    let sql = "INSERT INTO ~/Documents VALUES ('file.txt')";
    let result = parse_sql(sql);

    assert!(result.is_err());
    match result {
        Err(ParserError::UnsupportedStatement(_)) => {}
        _ => panic!("Expected UnsupportedStatement error"),
    }
}

#[test]
fn paths_are_preserved_and_home_is_expanded() {
    for path in [
        ".",
        "..",
        "./src",
        "../src",
        "/",
        "/var/log",
        "src/sub-dir",
        "123",
        "C:/Users/Documents",
    ] {
        let sql = format!("SELECT * FROM {path}");
        let FileQuery::Select {
            path: actual,
            recursive,
            ..
        } = parse_sql(&sql).unwrap()
        else {
            panic!("SELECT expected")
        };
        assert_eq!(actual, std::path::PathBuf::from(path), "{sql}");
        assert!(!recursive);
    }
    for (path, expected) in [
        ("~", dirs::home_dir().unwrap()),
        ("~/Documents", dirs::home_dir().unwrap().join("Documents")),
    ] {
        let FileQuery::Select { path: actual, .. } =
            parse_sql(&format!("SELECT * FROM {path}")).unwrap()
        else {
            panic!("SELECT expected")
        };
        assert_eq!(actual, expected);
    }
}

#[test]
fn repeated_home_separators_never_discard_the_home_directory() {
    let expected = dirs::home_dir().unwrap().join("Documents");
    for path in ["~//Documents", "~///Documents", "\"~//Documents\""] {
        for sql in [
            format!("SELECT * FROM {path}"),
            format!("UPDATE {path} SET permissions = '600'"),
        ] {
            let query = parse_sql(&sql).unwrap();
            let actual = match query {
                FileQuery::Select { path, .. }
                | FileQuery::Update { path, .. }
                | FileQuery::Group { path, .. } => path,
            };
            assert_eq!(actual, expected, "{sql}");
        }
    }
}

#[test]
fn arbitrary_numbers_and_operators_are_preserved() {
    for (text, expected) in [
        ("=", ComparisonOperator::Eq),
        ("!=", ComparisonOperator::NotEq),
        ("<>", ComparisonOperator::NotEq),
        ("<", ComparisonOperator::Lt),
        ("<=", ComparisonOperator::LtEq),
        (">", ComparisonOperator::Gt),
        (">=", ComparisonOperator::GtEq),
    ] {
        for number in ["42", "-7", "+3", "1.5", "2e3"] {
            let FileQuery::Select {
                condition:
                    Some(FileCondition::Compare {
                        attribute,
                        operator,
                        value,
                    }),
                ..
            } = parse_sql(&format!("SELECT * FROM . WHERE size{text}{number}")).unwrap()
            else {
                panic!("comparison expected")
            };
            assert_eq!(attribute, FileAttribute::Size);
            assert_eq!(operator, expected);
            assert!(
                matches!(value, FileValue::Number(n) if (n - number.parse::<f64>().unwrap()).abs() < f64::EPSILON)
            );
        }
    }
}

#[test]
fn malformed_values_and_function_modifiers_are_rejected() {
    for condition in [
        "size > 1e999",
        "is_directory > true",
        "is_directory BETWEEN false AND true",
        "name = NULL",
        "name = size",
        "owner = 1",
        "is_symlink > true",
        "created > 'not a date'",
        "accessed BETWEEN 1 AND 2",
        "size LIKE '%'",
        "REGEXP(size, 'x')",
        "REGEXP(name)",
        "REGEXP(name, 'x', 'y')",
        "REGEXP(name, 'x') OVER ()",
        "REGEXP(name, 'x' ORDER BY size)",
        "REGEXP(name, 'x') = true",
        "name LIKE ANY ('x', 'y')",
    ] {
        assert!(
            parse_sql(&format!("SELECT * FROM . WHERE {condition}")).is_err(),
            "{condition}"
        );
    }
}

#[test]
fn bare_boolean_attributes_test_for_true() {
    let FileQuery::Select {
        condition: Some(FileCondition::Not(inner)),
        ..
    } = parse_sql("SELECT * FROM . WHERE NOT is_symlink").unwrap()
    else {
        panic!("expected NOT condition");
    };
    assert!(matches!(
        *inner,
        FileCondition::Compare {
            attribute: FileAttribute::IsSymlink,
            operator: ComparisonOperator::Eq,
            value: FileValue::Boolean(true),
        }
    ));
    for condition in ["name", "size", "is_directory AND size", "NOT owner"] {
        assert!(
            parse_sql(&format!("SELECT * FROM . WHERE {condition}")).is_err(),
            "{condition}"
        );
    }
}

#[test]
fn projections_keep_their_order() {
    let FileQuery::Select { attributes, .. } =
        parse_sql("SELECT Path, size, IS_SYMLINK FROM .").unwrap()
    else {
        panic!("expected SELECT");
    };
    assert_eq!(
        attributes,
        [
            FileAttribute::Path,
            FileAttribute::Size,
            FileAttribute::IsSymlink
        ]
    );
    assert!(matches!(
        parse_sql("SELECT name, name FROM ."),
        Err(crate::sql::parser::ParserError::DuplicateColumn(column)) if column == "name"
    ));
}

#[test]
fn name_assignments_parse_to_expressions() {
    let name = || Box::new(NameExpr::Attribute(FileAttribute::Name));
    let parse = |expr: &str| {
        let FileQuery::Update { updates, .. } =
            parse_sql(&format!("UPDATE . SET name = {expr}")).unwrap()
        else {
            panic!("Expected UPDATE query");
        };
        assert_eq!(updates[0].attribute, FileAttribute::Name);
        updates[0].value.clone()
    };

    assert_eq!(
        parse("REPLACE(name, '.jpeg', '.jpg')"),
        UpdateValue::Name(NameExpr::Replace {
            expr: name(),
            from: ".jpeg".into(),
            to: ".jpg".into(),
        })
    );
    // Nested concatenations flatten, whichever syntax builds them.
    assert_eq!(
        parse("'old_' || (name || '.' || EXTENSION)"),
        parse("concat('old_', name, CONCAT('.', extension))")
    );
    assert_eq!(
        parse("'old_' || name || '.bak'"),
        UpdateValue::Name(NameExpr::Concat(vec![
            NameExpr::Literal("old_".into()),
            NameExpr::Attribute(FileAttribute::Name),
            NameExpr::Literal(".bak".into()),
        ]))
    );
    assert_eq!(
        parse("LOWER(UPPER(name))"),
        UpdateValue::Name(NameExpr::Lower(Box::new(NameExpr::Upper(name()))))
    );
    for expr in [
        "size",
        "path",
        "1",
        "REPLACE(name, '', 'x')",
        "REPLACE(name, extension, 'x')",
        "REPLACE(name, 'a')",
        "CONCAT()",
        "TRIM(name)",
        "LOWER(name, name)",
        "name + 'x'",
        "CONCAT(DISTINCT name)",
    ] {
        assert!(
            parse_sql(&format!("UPDATE . SET name = {expr}")).is_err(),
            "accepted {expr}"
        );
    }
}

#[test]
fn aggregates_and_group_by_build_a_group_query() {
    use crate::sql::ast::{AggregateFunction, Column, GroupOrder};

    let FileQuery::Group {
        columns,
        group_by,
        order_by,
        limit,
        ..
    } = parse_sql(
        "SELECT extension, COUNT(*), sum(size) FROM . GROUP BY extension \
         ORDER BY COUNT(*) DESC, extension LIMIT 3",
    )
    .unwrap()
    else {
        panic!("expected GROUP query");
    };
    let count = Column::Aggregate {
        function: AggregateFunction::Count,
        attribute: None,
    };
    assert_eq!(
        columns,
        [
            Column::Attribute(FileAttribute::Extension),
            count.clone(),
            Column::Aggregate {
                function: AggregateFunction::Sum,
                attribute: Some(FileAttribute::Size),
            },
        ]
    );
    assert_eq!(group_by, [FileAttribute::Extension]);
    assert_eq!(
        order_by,
        [
            GroupOrder {
                column: count,
                descending: true
            },
            GroupOrder {
                column: Column::Attribute(FileAttribute::Extension),
                descending: false
            },
        ]
    );
    assert_eq!(limit, Some(3));

    // An aggregate alone groups every file together.
    assert!(matches!(
        parse_sql("SELECT MAX(modified) FROM .").unwrap(),
        FileQuery::Group { ref group_by, .. } if group_by.is_empty()
    ));
}

#[test]
fn malformed_grouping_is_rejected() {
    for sql in [
        "SELECT * FROM . GROUP BY extension",
        "SELECT name, COUNT(*) FROM .",
        "SELECT name FROM . GROUP BY extension",
        "SELECT extension FROM . GROUP BY extension ORDER BY name",
        "SELECT name FROM . ORDER BY COUNT(*)",
        "SELECT SUM(name) FROM .",
        "SELECT AVG(permissions) FROM .",
        "SELECT SUM(*) FROM .",
        "SELECT COUNT(DISTINCT name) FROM .",
        "SELECT COUNT(name, size) FROM .",
        "SELECT MEDIAN(size) FROM .",
        "SELECT COUNT(*), COUNT(*) FROM .",
        "SELECT extension FROM . GROUP BY extension, extension",
        "SELECT extension FROM . GROUP BY",
        "SELECT COUNT(*) FROM . GROUP BY extension HAVING COUNT(*) > 1",
        "UPDATE . SET permissions = '600' GROUP BY extension",
    ] {
        assert!(parse_sql(sql).is_err(), "accepted {sql}");
    }
    assert!(matches!(
        parse_sql("SELECT name, COUNT(*) FROM ."),
        Err(ParserError::UngroupedColumn(column)) if column == "name"
    ));
}

#[test]
fn a_given_home_replaces_the_current_users_for_tilde() {
    let home = std::path::Path::new("/home/ada");
    for (sql, expected) in [
        ("UPDATE ~ SET owner = 'ada'", home.to_path_buf()),
        ("SELECT * FROM ~/notes", home.join("notes")),
        ("SELECT * FROM /tmp", std::path::PathBuf::from("/tmp")),
    ] {
        let actual = match parse_sql_with_home(sql, Some(home)).unwrap() {
            FileQuery::Select { path, .. }
            | FileQuery::Update { path, .. }
            | FileQuery::Group { path, .. } => path,
        };
        assert_eq!(actual, expected, "{sql}");
    }
}

#[test]
fn tilde_without_a_home_is_an_invalid_path() {
    assert!(matches!(
        parse_sql_with_home("SELECT * FROM ~/notes", None),
        Err(ParserError::InvalidPath(path)) if path == "~/notes"
    ));
    assert!(parse_sql_with_home("SELECT * FROM .", None).is_ok());
}
