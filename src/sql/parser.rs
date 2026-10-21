//! Parsing and validation for the supported file-query SQL subset.
//!
//! Supports `SELECT * | attribute, ... FROM path [WHERE condition] [ORDER BY attribute
//! [ASC|DESC], ...] [LIMIT n]` and
//! `UPDATE path SET permissions = '640', owner = 'user', name = expr [WHERE condition]`.
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
    AggregateFunction, Column, ComparisonOperator, FileAttribute, FileAttributeUpdate,
    FileCondition, FileQuery, FileValue, GroupOrder, NameExpr, OrderBy, UpdateValue,
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
    #[error("Attribute assigned more than once: {0}")]
    DuplicateAssignment(String),
    #[error("{0} must appear in GROUP BY or inside an aggregate")]
    UngroupedColumn(String),
    #[error("Column selected more than once: {0}")]
    DuplicateColumn(String),
    #[error("LIMIT {0} is too large")]
    InvalidLimit(u64),
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
    // `None` stands for `*`.
    let mut columns = None;
    if select {
        columns = parse_select_list(&mut parser)?;
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
    // Updates have no grouping, order or limit; the clauses stay unconsumed
    // and are rejected by the complete-query check.
    let group_by = if select && parser.parse_keywords(&[Keyword::GROUP, Keyword::BY]) {
        parse_group_by(&mut parser)?
    } else {
        Vec::new()
    };
    let order_by = if select && parser.parse_keywords(&[Keyword::ORDER, Keyword::BY]) {
        parse_order_by(&mut parser)?
    } else {
        Vec::new()
    };
    let limit = if select && parser.parse_keyword(Keyword::LIMIT) {
        let limit = parser.parse_literal_uint()?;
        Some(usize::try_from(limit).map_err(|_| ParserError::InvalidLimit(limit))?)
    } else {
        None
    };
    let _ = parser.consume_token(&Token::SemiColon);
    parser.expect_token(&Token::EOF)?;
    if !select {
        return Ok(FileQuery::Update {
            path,
            recursive,
            updates,
            condition,
        });
    }
    select_query(SelectParts {
        path,
        recursive,
        columns,
        condition,
        group_by,
        order_by,
        limit,
    })
}

/// The clauses of a `SELECT`, before deciding whether it groups.
struct SelectParts {
    path: PathBuf,
    recursive: bool,
    columns: Option<Vec<Column>>,
    condition: Option<FileCondition>,
    group_by: Vec<FileAttribute>,
    order_by: Vec<GroupOrder>,
    limit: Option<usize>,
}

/// Builds a grouped query when there is a `GROUP BY` or an aggregate in the
/// select list, and a plain `SELECT` otherwise.
fn select_query(parts: SelectParts) -> Result<FileQuery> {
    let SelectParts {
        path,
        recursive,
        columns,
        condition,
        group_by,
        order_by,
        limit,
    } = parts;
    let aggregates = |columns: &[Column]| {
        columns
            .iter()
            .any(|column| matches!(column, Column::Aggregate { .. }))
    };
    let grouped = !group_by.is_empty() || columns.as_deref().is_some_and(aggregates);
    if grouped {
        let columns = columns.ok_or_else(|| {
            ParserError::UnsupportedExpression("SELECT * with GROUP BY or an aggregate".to_owned())
        })?;
        for column in columns.iter().chain(order_by.iter().map(|key| &key.column)) {
            if let Column::Attribute(attribute) = column
                && !group_by.contains(attribute)
            {
                return Err(ParserError::UngroupedColumn(attribute.name().to_owned()));
            }
        }
        return Ok(FileQuery::Group {
            path,
            recursive,
            columns,
            condition,
            group_by,
            order_by,
            limit,
        });
    }
    let order_by = order_by
        .into_iter()
        .map(|key| match key.column {
            Column::Attribute(attribute) => Ok(OrderBy {
                attribute,
                descending: key.descending,
            }),
            column @ Column::Aggregate { .. } => Err(ParserError::UnsupportedExpression(format!(
                "ORDER BY {} without GROUP BY or an aggregate in the select list",
                column.label()
            ))),
        })
        .collect::<Result<Vec<_>>>()?;
    let attributes = columns.map_or_else(
        || vec![FileAttribute::All],
        |columns| {
            columns
                .into_iter()
                .filter_map(|column| match column {
                    Column::Attribute(attribute) => Some(attribute),
                    Column::Aggregate { .. } => None,
                })
                .collect()
        },
    );
    Ok(FileQuery::Select {
        path,
        recursive,
        attributes,
        condition,
        order_by,
        limit,
    })
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

/// Reads `*`, returned as `None`, or a list of distinct columns. `*` stands
/// alone.
fn parse_select_list(parser: &mut Parser<'_>) -> Result<Option<Vec<Column>>> {
    if parser.consume_token(&Token::Mul) {
        return Ok(None);
    }
    let mut columns: Vec<Column> = Vec::new();
    loop {
        let column = column(&parser.parse_expr()?)?;
        if columns.contains(&column) {
            return Err(ParserError::DuplicateColumn(column.label()));
        }
        columns.push(column);
        if !parser.consume_token(&Token::Comma) {
            return Ok(Some(columns));
        }
    }
}

fn parse_group_by(parser: &mut Parser<'_>) -> Result<Vec<FileAttribute>> {
    let mut attributes = Vec::new();
    loop {
        let attribute = parse_attribute(&Expr::Identifier(parser.parse_identifier()?))?;
        if attributes.contains(&attribute) {
            return Err(ParserError::DuplicateColumn(attribute.name().to_owned()));
        }
        attributes.push(attribute);
        if !parser.consume_token(&Token::Comma) {
            return Ok(attributes);
        }
    }
}

fn parse_order_by(parser: &mut Parser<'_>) -> Result<Vec<GroupOrder>> {
    let mut keys = Vec::new();
    loop {
        let column = column(&parser.parse_expr()?)?;
        let descending =
            parser.parse_one_of_keywords(&[Keyword::ASC, Keyword::DESC]) == Some(Keyword::DESC);
        keys.push(GroupOrder { column, descending });
        if !parser.consume_token(&Token::Comma) {
            return Ok(keys);
        }
    }
}

/// Converts a select-list or sort item: an attribute or an aggregate call.
fn column(expr: &Expr) -> Result<Column> {
    let Expr::Function(function) = expr else {
        return Ok(Column::Attribute(parse_attribute(expr)?));
    };
    let Some((name, args)) = plain_args(function) else {
        return Err(unsupported(expr));
    };
    let aggregate = match name.as_str() {
        "COUNT" => AggregateFunction::Count,
        "SUM" => AggregateFunction::Sum,
        "AVG" => AggregateFunction::Avg,
        "MIN" => AggregateFunction::Min,
        "MAX" => AggregateFunction::Max,
        _ => return Err(unsupported(expr)),
    };
    let attribute = match args {
        [FunctionArg::Unnamed(FunctionArgExpr::Wildcard)]
            if aggregate == AggregateFunction::Count =>
        {
            None
        }
        [FunctionArg::Unnamed(FunctionArgExpr::Expr(arg))] => Some(parse_attribute(arg)?),
        _ => return Err(unsupported(expr)),
    };
    if matches!(aggregate, AggregateFunction::Sum | AggregateFunction::Avg)
        // Adding permission bits means nothing; size is the only quantity.
        && attribute != Some(FileAttribute::Size)
    {
        return Err(unsupported(expr));
    }
    Ok(Column::Aggregate {
        function: aggregate,
        attribute,
    })
}

fn parse_updates(parser: &mut Parser<'_>) -> Result<Vec<FileAttributeUpdate>> {
    parser.expect_keyword(Keyword::SET)?;
    let mut updates: Vec<FileAttributeUpdate> = Vec::new();
    loop {
        let name = parser.parse_identifier()?;
        let attribute = match name.value.to_ascii_lowercase().as_str() {
            "permissions" => FileAttribute::Permissions,
            "owner" => FileAttribute::Owner,
            "name" => FileAttribute::Name,
            _ => return Err(ParserError::UnsupportedAttribute(name.value)),
        };
        if updates.iter().any(|update| update.attribute == attribute) {
            return Err(ParserError::DuplicateAssignment(
                attribute.name().to_owned(),
            ));
        }
        parser.expect_token(&Token::Eq)?;
        let expr = parser.parse_expr()?;
        let value = match attribute {
            FileAttribute::Permissions => UpdateValue::Literal(format!("{:o}", octal_mode(&expr)?)),
            // The executor checks each computed name before any file changes.
            FileAttribute::Name => UpdateValue::Name(name_expr(&expr)?),
            // The executor resolves the user, so an unknown one fails before
            // any file changes.
            _ => match string_literal(&expr) {
                Ok(name) if !name.is_empty() => UpdateValue::Literal(name),
                _ => return Err(invalid_value(&attribute, &expr)),
            },
        };
        updates.push(FileAttributeUpdate { attribute, value });
        if !parser.consume_token(&Token::Comma) {
            return Ok(updates);
        }
    }
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
        Expr::Identifier(_) => bare_boolean(expr),
        _ => Err(unsupported(expr)),
    }
}

/// A bare boolean attribute, as in `WHERE is_directory`, tests for true.
fn bare_boolean(expr: &Expr) -> Result<FileCondition> {
    match parse_attribute(expr)? {
        attribute @ (FileAttribute::IsDirectory
        | FileAttribute::IsExecutable
        | FileAttribute::IsSymlink) => Ok(FileCondition::Compare {
            attribute,
            operator: ComparisonOperator::Eq,
            value: FileValue::Boolean(true),
        }),
        _ => Err(unsupported(expr)),
    }
}

/// Booleans have no order, and a mode is a bit set rather than a magnitude.
fn equality_only(attribute: &FileAttribute) -> bool {
    matches!(
        attribute,
        FileAttribute::IsDirectory
            | FileAttribute::IsExecutable
            | FileAttribute::IsSymlink
            | FileAttribute::Permissions
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
    if let Some((name, args)) = plain_call(function)
        && let [attribute, pattern] = args.as_slice()
        && name == "REGEXP"
    {
        return regexp(attribute, pattern);
    }
    Err(ParserError::UnsupportedExpression(function.to_string()))
}

/// The upper-case name and positional arguments of a plain call such as
/// `F(a, b)`, or `None` if it uses any other function syntax.
fn plain_call(function: &Function) -> Option<(String, Vec<&Expr>)> {
    let (name, args) = plain_args(function)?;
    let args = args
        .iter()
        .map(|arg| match arg {
            FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) => Some(expr),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some((name, args))
}

/// The upper-case name and raw arguments of a call with no modifiers such as
/// `DISTINCT`, `FILTER`, `OVER` or `ORDER BY`.
fn plain_args(function: &Function) -> Option<(String, &[FunctionArg])> {
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
        || *uses_odbc_syntax
        || !matches!(parameters, FunctionArguments::None)
        || filter.is_some()
        || null_treatment.is_some()
        || over.is_some()
        || !within_group.is_empty()
    {
        return None;
    }
    let FunctionArguments::List(list) = args else {
        return None;
    };
    if list.duplicate_treatment.is_some() || !list.clauses.is_empty() {
        return None;
    }
    Some((name.to_string().to_ascii_uppercase(), &list.args))
}

/// Converts the right-hand side of `SET name = ...`.
fn name_expr(expr: &Expr) -> Result<NameExpr> {
    match expr {
        Expr::Nested(inner) => name_expr(inner),
        Expr::Value(_) => Ok(NameExpr::Literal(string_literal(expr)?)),
        Expr::Identifier(_) => match parse_attribute(expr)? {
            attribute @ (FileAttribute::Name | FileAttribute::Extension) => {
                Ok(NameExpr::Attribute(attribute))
            }
            _ => Err(unsupported(expr)),
        },
        Expr::BinaryOp {
            left,
            op: BinaryOperator::StringConcat,
            right,
        } => Ok(concat([name_expr(left)?, name_expr(right)?])),
        Expr::Function(function) => match plain_call(function) {
            Some((name, args)) if name == "CONCAT" && !args.is_empty() => Ok(concat(
                args.into_iter()
                    .map(name_expr)
                    .collect::<Result<Vec<_>>>()?,
            )),
            Some((name, args)) if name == "REPLACE" && args.len() == 3 => {
                let from = string_literal(args[1])?;
                if from.is_empty() {
                    return Err(unsupported(args[1]));
                }
                Ok(NameExpr::Replace {
                    expr: Box::new(name_expr(args[0])?),
                    from,
                    to: string_literal(args[2])?,
                })
            }
            Some((name, args)) if name == "LOWER" && args.len() == 1 => {
                Ok(NameExpr::Lower(Box::new(name_expr(args[0])?)))
            }
            Some((name, args)) if name == "UPPER" && args.len() == 1 => {
                Ok(NameExpr::Upper(Box::new(name_expr(args[0])?)))
            }
            _ => Err(unsupported(expr)),
        },
        _ => Err(unsupported(expr)),
    }
}

/// Joins parts, flattening nested concatenations.
fn concat(parts: impl IntoIterator<Item = NameExpr>) -> NameExpr {
    let mut flat = Vec::new();
    for part in parts {
        match part {
            NameExpr::Concat(inner) => flat.extend(inner),
            part => flat.push(part),
        }
    }
    NameExpr::Concat(flat)
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
        "created" => Ok(FileAttribute::Created),
        "accessed" => Ok(FileAttribute::Accessed),
        "permissions" => Ok(FileAttribute::Permissions),
        "mode" => Ok(FileAttribute::Mode),
        "owner" => Ok(FileAttribute::Owner),
        "is_directory" => Ok(FileAttribute::IsDirectory),
        "is_executable" => Ok(FileAttribute::IsExecutable),
        "is_symlink" => Ok(FileAttribute::IsSymlink),
        _ => Err(ParserError::UnsupportedAttribute(ident.value.clone())),
    }
}

fn string_attribute(expr: &Expr) -> Result<FileAttribute> {
    let attribute = parse_attribute(expr)?;
    if matches!(
        attribute,
        FileAttribute::Name
            | FileAttribute::Path
            | FileAttribute::Extension
            | FileAttribute::Owner
            | FileAttribute::Mode
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
        | FileAttribute::Owner
        | FileAttribute::Mode
        | FileAttribute::Modified
        | FileAttribute::Created
        | FileAttribute::Accessed => {
            let mut value = string_literal(expr)?;
            if *attribute == FileAttribute::Extension {
                value = value.strip_prefix('.').unwrap_or(&value).to_owned();
            }
            let is_time = matches!(
                attribute,
                FileAttribute::Modified | FileAttribute::Created | FileAttribute::Accessed
            );
            if is_time
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
        FileAttribute::IsDirectory | FileAttribute::IsExecutable | FileAttribute::IsSymlink => {
            if let Expr::Value(value) = expr
                && let Value::Boolean(b) = value.value
            {
                return Ok(FileValue::Boolean(b));
            }
            Err(invalid_value(attribute, expr))
        }
        FileAttribute::All => Err(ParserError::UnsupportedAttribute(format!("{attribute:?}"))),
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
