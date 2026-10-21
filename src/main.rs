//! fmql - A fast and feature-rich file manager written in Rust.
//!
//! This crate provides a command-line tool for managing files using a SQL-like query language.

use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use fmql::FMQLError;
use fmql::output::{columns_json, render_columns, render_table, render_text, table_json};
use fmql::sql::ast::{FileAttribute, FileQuery};
use fmql::sql::{execute_grouped, execute_query, parse_sql};

/// Command-line arguments for the SQL mode
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct SqlCommand {
    /// SQL query to execute
    #[arg(
        help = "SQL query to execute (e.g., \"SELECT * FROM ~/Documents WHERE extension = 'txt'\")"
    )]
    query: String,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = Format::Text)]
    format: Format,
}

/// How results are written to standard output.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum Format {
    /// One `ls -l` style line per entry.
    Text,
    /// A JSON array of entries.
    Json,
}

/// Command-line arguments for the main application
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct AppArgs {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Query files using SQL-like syntax
    Sql(SqlCommand),
}

fn main() -> ExitCode {
    let AppArgs {
        command: Command::Sql(args),
    } = AppArgs::parse();
    match run_sql_mode(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

/// Run the application in SQL mode
fn run_sql_mode(args: &SqlCommand) -> Result<(), Box<dyn std::error::Error>> {
    let query = parse_sql(&args.query).map_err(FMQLError::from)?;
    if let FileQuery::Group { .. } = query {
        let table = execute_grouped(&query).map_err(FMQLError::from)?;
        match args.format {
            Format::Json => println!("{}", serde_json::to_string_pretty(&table_json(&table))?),
            Format::Text => print!("{}", render_table(&table)),
        }
        return Ok(());
    }
    let results = execute_query(&query).map_err(FMQLError::from)?;
    let columns = match &query {
        FileQuery::Select { attributes, .. } if attributes != &[FileAttribute::All] => {
            Some(attributes.as_slice())
        }
        _ => None,
    };
    match (args.format, columns) {
        (Format::Json, None) => println!("{}", serde_json::to_string_pretty(&results)?),
        (Format::Json, Some(columns)) => println!(
            "{}",
            serde_json::to_string_pretty(&columns_json(&results, columns))?
        ),
        (Format::Text, None) => print!("{}", render_text(&results)),
        (Format::Text, Some(columns)) => print!("{}", render_columns(&results, columns)),
    }
    Ok(())
}
