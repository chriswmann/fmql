//! Parsing and validation for the supported file-query SQL subset.
//!
//! Supports `SELECT * FROM path [WHERE condition]` and
//! `UPDATE path SET permissions = '640' [WHERE condition]`.
//! Prefix either with `WITH RECURSIVE` to include descendants. Double-quote
//! paths containing spaces or SQL punctuation. Single-quote string literals,
//! doubling quotes for embedded apostrophes.
//!
//! The statement envelope deliberately accepts only operations the executor
//! implements. sqlparser handles tokenisation and expressions, including
//! precedence, parentheses, comments and escaping. Every token must be consumed.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use sqlparser::ast::{
    BinaryOperator, Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments, UnaryOperator,
    Value,
};
use sqlparser::keywords::Keyword;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::Token;
use thiserror::Error;

use crate::sql::ast::{
    ComparisonOperator, FileAttribute, FileAttributeUpdate, FileCondition, FileQuery, FileValue,
};
use crate::sql::dialect::FileDialect;

/// Errors produced before a file query can be executed.
#[derive(Error, Debug)]
pub enum ParserError {
    #[error("SQL parser error: {0}")]
    Sql(#[from] sqlparser::parser::ParserError),
    #[error("Unsupported SQL statement: {0}")]
    UnsupportedStatement(String),
    #[error("Unsupported SQL expression: {0}")]
    UnsupportedExpression(String),
    #[error("Unsupported file attribute: {0}")]
    UnsupportedAttribute(String),
    #[error("Invalid file path: {0}")]
    InvalidPath(String),
    #[error("Missing required clause: {0}")]
    MissingClause(String),
    #[error("Invalid value for {attribute:?}: {value}")]
    InvalidValue {
        attribute: FileAttribute,
        value: String,
    },
    #[error("Invalid regular expression: {0}")]
    InvalidRegex(#[from] regex::Error),
}

pub type Result<T> = std::result::Result<T, ParserError>;

/// Parse one complete query, rejecting unsupported syntax rather than weakening
/// its predicate. Validation finishes before filesystem traversal or mutation.
///
/// # Errors
/// Returns an error for malformed or unsupported SQL, unavailable attributes,
/// incompatible literals, invalid patterns, or paths that cannot be expanded.
pub fn parse_sql(sql: &str) -> Result<FileQuery> {
    let dialect = FileDialect::default();
    let mut parser = Parser::new(&dialect).try_with_sql(sql)?;
    let recursive = parser.parse_keywords(&[Keyword::WITH, Keyword::RECURSIVE]);
    let select = parser.parse_keyword(Keyword::SELECT);
    if select {
        // Projection is not implemented by the executor; do not silently ignore it.
        parser.expect_token(&Token::Mul)?;
        parser.expect_keyword(Keyword::FROM)?;
    } else if !parser.parse_keyword(Keyword::UPDATE) {
        return Err(ParserError::UnsupportedStatement(sql.to_owned()));
    }
    let path = parse_path(&mut parser)?;
    let updates = if select {
        Vec::new()
    } else {
        parse_updates(&mut parser)?
    };
    let condition = if parser.parse_keyword(Keyword::WHERE) {
        Some(convert_condition(&parser.parse_expr()?)?)
    } else {
        None
    };
    let _ = parser.consume_token(&Token::SemiColon);
    parser.expect_token(&Token::EOF)?;
    if select {
        Ok(FileQuery::Select {
            path,
            recursive,
            attributes: vec![FileAttribute::All],
            condition,
        })
    } else {
        Ok(FileQuery::Update {
            path,
            recursive,
            updates,
            condition,
        })
    }
}

fn parse_path(parser: &mut Parser<'_>) -> Result<PathBuf> {
    let first = parser.next_token();
    if first.token == Token::EOF {
        return Err(ParserError::MissingClause("directory path".to_owned()));
    }
    let mut end = first.span.end;
    let quoted = matches!(&first.token, Token::Word(word) if word.quote_style.is_some());
    let mut path = path_token(&first.token)
        .ok_or_else(|| ParserError::InvalidPath(first.token.to_string()))?;
    // sqlparser tokenises leading '/' and '.' as operators even for FileDialect.
    // Join only adjacent path tokens; never swallow whitespace or SQL clauses.
    while !quoted && parser.peek_token().span.start == end {
        let next = parser.peek_token();
        if matches!(&next.token, Token::Word(word) if word.quote_style.is_some()) {
            break;
        }
        let part = match &next.token {
            Token::Word(word) => Some(word.value.clone()),
            token => path_token(token),
        };
        let Some(part) = part else {
            break;
        };
        path.push_str(&part);
        end = parser.next_token().span.end;
    }
    if path.is_empty() || path.contains('\0') {
        return Err(ParserError::InvalidPath(path));
    }
    if path == "~" || path.starts_with("~/") {
        let home = dirs::home_dir().ok_or_else(|| ParserError::InvalidPath(path.clone()))?;
        return Ok(if path == "~" {
            home
        } else {
            home.join(path[2..].trim_start_matches('/'))
        });
    }
    if path.starts_with('~') {
        return Err(ParserError::InvalidPath(path));
    }
    Ok(PathBuf::from(path))
}

fn path_token(token: &Token) -> Option<String> {
    match token {
        // Directory names such as `data` or `cache` are SQL keywords. Only the
        // clause keywords that may follow a path mark a missing one.
        Token::Word(word)
            if word.quote_style.is_some()
                || !matches!(word.keyword, Keyword::WHERE | Keyword::SET) =>
        {
            Some(word.value.clone())
        }
        Token::Div | Token::Period | Token::Minus | Token::Tilde | Token::Number(_, false) => {
            Some(token.to_string())
        }
        _ => None,
    }
}

fn parse_updates(parser: &mut Parser<'_>) -> Result<Vec<FileAttributeUpdate>> {
    parser.expect_keyword(Keyword::SET)?;
    let attribute = parser.parse_identifier()?;
    if !attribute.value.eq_ignore_ascii_case("permissions") {
        return Err(ParserError::UnsupportedAttribute(attribute.value));
    }
    parser.expect_token(&Token::Eq)?;
    let mode = octal_mode(&parser.parse_expr()?)?;
    // Only one writable attribute exists. Additional assignments remain
    // unconsumed and are rejected by the complete-query check.
    Ok(vec![FileAttributeUpdate {
        attribute: FileAttribute::Permissions,
        value: format!("{mode:o}"),
    }])
}

/// Read a permission literal as octal digits, as `chmod` does. Filters and
/// updates share this so `644` means the same mode in both.
fn octal_mode(expr: &Expr) -> Result<u32> {
    if let Expr::Value(value) = expr
        && let Value::SingleQuotedString(digits) | Value::Number(digits, false) = &value.value
        && digits.chars().all(|c| matches!(c, '0'..='7'))
        && let Ok(mode) = u32::from_str_radix(digits, 8)
        && mode <= 0o7777
    {
        return Ok(mode);
    }
    Err(invalid_value(&FileAttribute::Permissions, expr))
}

fn convert_condition(expr: &Expr) -> Result<FileCondition> {
    match expr {
        Expr::Nested(inner) => convert_condition(inner),
        Expr::UnaryOp {
            op: UnaryOperator::Not,
            expr,
        } => Ok(FileCondition::Not(Box::new(convert_condition(expr)?))),
        Expr::BinaryOp {
            left,
            op: BinaryOperator::And,
            right,
        } => Ok(FileCondition::And(
            Box::new(convert_condition(left)?),
            Box::new(convert_condition(right)?),
        )),
        Expr::BinaryOp {
            left,
            op: BinaryOperator::Or,
            right,
        } => Ok(FileCondition::Or(
            Box::new(convert_condition(left)?),
            Box::new(convert_condition(right)?),
        )),
        Expr::BinaryOp { left, op, right } => {
            let operator = comparison_operator(op)?;
            let attribute = parse_attribute(left)?;
            if equality_only(&attribute)
                && !matches!(operator, ComparisonOperator::Eq | ComparisonOperator::NotEq)
            {
                return Err(unsupported(expr));
            }
            let value = parse_value(&attribute, right)?;
            Ok(FileCondition::Compare {
                attribute,
                operator,
                value,
            })
        }
        Expr::Like {
            negated,
            any: false,
            expr,
            pattern,
            escape_char: None,
        }
        | Expr::ILike {
            negated,
            any: false,
            expr,
            pattern,
            escape_char: None,
        } => {
            let attribute = string_attribute(expr)?;
            let pattern = string_literal(pattern)?;
            Ok(negate(
                FileCondition::Like {
                    attribute,
                    pattern,
                    case_sensitive: false,
                },
                *negated,
            ))
        }
        Expr::Between {
            expr,
            negated,
            low,
            high,
        } => {
            let attribute = parse_attribute(expr)?;
            if equality_only(&attribute) {
                return Err(unsupported(expr));
            }
            let lower = parse_value(&attribute, low)?;
            let upper = parse_value(&attribute, high)?;
            Ok(negate(
                FileCondition::Between {
                    attribute,
                    lower,
                    upper,
                },
                *negated,
            ))
        }
        Expr::RLike {
            negated,
            expr,
            pattern,
            ..
        } => Ok(negate(regexp(expr, pattern)?, *negated)),
        Expr::Function(function) => convert_function(function),
        _ => Err(unsupported(expr)),
    }
}

/// Booleans have no order, and a mode is a bit set rather than a magnitude.
fn equality_only(attribute: &FileAttribute) -> bool {
    matches!(
        attribute,
        FileAttribute::IsDirectory | FileAttribute::IsExecutable | FileAttribute::Permissions
    )
}

fn comparison_operator(operator: &BinaryOperator) -> Result<ComparisonOperator> {
    match operator {
        BinaryOperator::Eq => Ok(ComparisonOperator::Eq),
        BinaryOperator::NotEq => Ok(ComparisonOperator::NotEq),
        BinaryOperator::Lt => Ok(ComparisonOperator::Lt),
        BinaryOperator::LtEq => Ok(ComparisonOperator::LtEq),
        BinaryOperator::Gt => Ok(ComparisonOperator::Gt),
        BinaryOperator::GtEq => Ok(ComparisonOperator::GtEq),
        _ => Err(ParserError::UnsupportedExpression(operator.to_string())),
    }
}

fn convert_function(function: &Function) -> Result<FileCondition> {
    let Function {
        name,
        uses_odbc_syntax,
        parameters,
        args,
        filter,
        null_treatment,
        over,
        within_group,
    } = function;
    if name.0.len() != 1
        || !name.to_string().eq_ignore_ascii_case("REGEXP")
        || *uses_odbc_syntax
        || !matches!(parameters, FunctionArguments::None)
        || filter.is_some()
        || null_treatment.is_some()
        || over.is_some()
        || !within_group.is_empty()
    {
        return Err(ParserError::UnsupportedExpression(function.to_string()));
    }
    if let FunctionArguments::List(list) = args
        && list.duplicate_treatment.is_none()
        && list.clauses.is_empty()
        && let [
            FunctionArg::Unnamed(FunctionArgExpr::Expr(attribute)),
            FunctionArg::Unnamed(FunctionArgExpr::Expr(pattern)),
        ] = list.args.as_slice()
    {
        return regexp(attribute, pattern);
    }
    Err(ParserError::UnsupportedExpression(function.to_string()))
}

fn regexp(attribute: &Expr, pattern: &Expr) -> Result<FileCondition> {
    let attribute = string_attribute(attribute)?;
    let pattern = string_literal(pattern)?;
    regex::Regex::new(&pattern)?;
    Ok(FileCondition::Regexp { attribute, pattern })
}

fn negate(condition: FileCondition, negated: bool) -> FileCondition {
    if negated {
        FileCondition::Not(Box::new(condition))
    } else {
        condition
    }
}

fn parse_attribute(expr: &Expr) -> Result<FileAttribute> {
    let Expr::Identifier(ident) = expr else {
        return Err(unsupported(expr));
    };
    match ident.value.to_ascii_lowercase().as_str() {
        "name" => Ok(FileAttribute::Name),
        "path" => Ok(FileAttribute::Path),
        "size" => Ok(FileAttribute::Size),
        "extension" => Ok(FileAttribute::Extension),
        "modified" => Ok(FileAttribute::Modified),
        "permissions" => Ok(FileAttribute::Permissions),
        "is_directory" => Ok(FileAttribute::IsDirectory),
        "is_executable" => Ok(FileAttribute::IsExecutable),
        _ => Err(ParserError::UnsupportedAttribute(ident.value.clone())),
    }
}

fn string_attribute(expr: &Expr) -> Result<FileAttribute> {
    let attribute = parse_attribute(expr)?;
    if matches!(
        attribute,
        FileAttribute::Name | FileAttribute::Path | FileAttribute::Extension
    ) {
        Ok(attribute)
    } else {
        Err(unsupported(expr))
    }
}

fn string_literal(expr: &Expr) -> Result<String> {
    if let Expr::Value(value) = expr
        && let Value::SingleQuotedString(s) = &value.value
    {
        return Ok(s.clone());
    }
    Err(unsupported(expr))
}

fn parse_value(attribute: &FileAttribute, expr: &Expr) -> Result<FileValue> {
    match attribute {
        FileAttribute::Name
        | FileAttribute::Path
        | FileAttribute::Extension
        | FileAttribute::Modified => {
            let mut value = string_literal(expr)?;
            if *attribute == FileAttribute::Extension {
                value = value.strip_prefix('.').unwrap_or(&value).to_owned();
            }
            if *attribute == FileAttribute::Modified
                && value.parse::<DateTime<Utc>>().is_err()
                && format!("{value}T00:00:00Z")
                    .parse::<DateTime<Utc>>()
                    .is_err()
            {
                return Err(invalid_value(attribute, expr));
            }
            Ok(FileValue::String(value))
        }
        FileAttribute::Permissions => Ok(FileValue::Number(f64::from(octal_mode(expr)?))),
        FileAttribute::Size => {
            let (expr, sign) = match expr {
                Expr::UnaryOp {
                    op: UnaryOperator::Minus,
                    expr,
                } => (expr.as_ref(), -1.0),
                Expr::UnaryOp {
                    op: UnaryOperator::Plus,
                    expr,
                } => (expr.as_ref(), 1.0),
                _ => (expr, 1.0),
            };
            if let Expr::Value(value) = expr
                && let Value::Number(n, false) = &value.value
                && let Ok(number) = n.parse::<f64>()
                && number.is_finite()
            {
                return Ok(FileValue::Number(sign * number));
            }
            Err(invalid_value(attribute, expr))
        }
        FileAttribute::IsDirectory | FileAttribute::IsExecutable => {
            if let Expr::Value(value) = expr
                && let Value::Boolean(b) = value.value
            {
                return Ok(FileValue::Boolean(b));
            }
            Err(invalid_value(attribute, expr))
        }
        _ => Err(ParserError::UnsupportedAttribute(format!("{attribute:?}"))),
    }
}

fn unsupported(expr: &Expr) -> ParserError {
    ParserError::UnsupportedExpression(expr.to_string())
}

fn invalid_value(attribute: &FileAttribute, expr: &Expr) -> ParserError {
    ParserError::InvalidValue {
        attribute: attribute.clone(),
        value: expr.to_string(),
    }
}

#[cfg(test)]
#[path = "parser_tests.rs"]
mod tests;
