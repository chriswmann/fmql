//! ls-rs - A fast and feature-rich file manager written in Rust.
//!
//! This crate provides a command-line tool for listing and managing files with various
//! features like sorting, grouping, and detailed file information. It also includes a
//! SQL-like query language for more powerful file management operations.

mod cli;
mod display;
mod error;
mod file;
mod sql;

use clap::{Parser, Subcommand};
use std::process;

use crate::cli::Args;
use crate::display::display_file_list;
use crate::file::list_directory;
use crate::sql::{parse_sql, execute_query};

/// Command-line arguments for the SQL mode
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct SqlCommand {
    /// SQL query to execute
    #[arg(help = "SQL query to execute (e.g., \"SELECT * FROM ~/Documents WHERE extension = '.txt'\"")]
    query: String,
    
    /// Output format (text or json)
    #[arg(short, long, default_value = "text")]
    format: String,
}

/// Command-line arguments for the main application
#[derive(Parser, Debug)]
#[command(author, version, about)]
struct AppArgs {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// List files using traditional flags
    Ls(Args),
    
    /// Query files using SQL-like syntax
    Sql(SqlCommand),
}

fn main() {
    let args = AppArgs::parse();
    
    match args.command {
        Some(Command::Ls(ls_args)) => {
            // Run in traditional ls mode
            run_ls_mode(ls_args);
        },
        Some(Command::Sql(sql_args)) => {
            // Run in SQL mode
            run_sql_mode(&sql_args);
        },
        None => {
            // Default to ls mode with default args
            run_ls_mode(Args::default());
        }
    }
}

/// Run the application in traditional ls mode
fn run_ls_mode(args: Args) {
    match args.validate() {
        Ok(()) => {
            match list_directory(&args) {
                Ok(files) => {
                    display_file_list(&files, &args);
                },
                Err(err) => {
                    eprintln!("Error: {}", err);
                    process::exit(1);
                }
            }
        },
        Err(err) => {
            eprintln!("Error: {}", err);
            process::exit(1);
        }
    }
}

/// Run the application in SQL mode
fn run_sql_mode(args: &SqlCommand) {
    match parse_sql(&args.query) {
        Ok(query) => {
            match execute_query(&query) {
                Ok(results) => {
                    match args.format.as_str() {
                        "json" => {
                            println!("{}", serde_json::to_string_pretty(&results).unwrap_or_else(|e| {
                                eprintln!("Error serializing results: {}", e);
                                process::exit(1);
                            }));
                        },
                        _ => {
                            // Default to text output
                            println!("{} results found:", results.len());
                            for result in &results {
                                println!("{}: {} bytes", result.path.display(), result.size);
                            }
                        }
                    }
                },
                Err(err) => {
                    eprintln!("Error executing query: {}", err);
                    process::exit(1);
                }
            }
        },
        Err(err) => {
            eprintln!("Error parsing SQL query: {}", err);
            process::exit(1);
        }
    }
} 