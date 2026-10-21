//! fmql - A fast and feature-rich file manager written in Rust.
//!
//! This crate provides a command-line tool for managing files using a SQL-like query language.

use std::error::Error;
use std::ffi::OsString;
use std::io::{BufRead, IsTerminal, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, ExitCode};

use clap::{Parser, Subcommand, ValueEnum};

use fmql::FMQLError;
use fmql::output::{columns_json, render_columns, render_table, render_text, table_json};
use fmql::sql::ast::{FileAttribute, FileQuery};
use fmql::sql::{execute_grouped, execute_query, parse_sql_with_home, plan_update};

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

    /// Show the entries an UPDATE would update, without changing anything
    #[arg(long)]
    dry_run: bool,

    /// Run a recursive UPDATE as root without asking first
    #[arg(short, long)]
    yes: bool,
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
    /// Rerun as root through sudo, or through PROGRAM, such as doas
    #[arg(
        long,
        value_name = "PROGRAM",
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "sudo"
    )]
    sudo: Option<String>,

    /// The directory `~` expands to, or none if empty; --sudo passes on the
    /// caller's home
    #[arg(long, hide = true, require_equals = true)]
    home: Option<OsString>,

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
        sudo,
        home,
        command: Command::Sql(args),
    } = AppArgs::parse();
    let home = match home {
        // The caller had no home, so `~` must not find root's instead.
        Some(home) if home.is_empty() => None,
        Some(home) => Some(PathBuf::from(home)),
        None => dirs::home_dir(),
    };
    if let Some(program) = sudo
        && !nix::unistd::geteuid().is_root()
    {
        let err = rerun_as_root(&program, home.as_deref());
        eprintln!("Could not run {program}: {err}");
        return ExitCode::FAILURE;
    }
    match run_sql_mode(&args, home.as_deref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

/// Replaces this process with `program` running fmql with the same arguments,
/// less `--sudo`, returning only if that fails. Root's environment usually
/// names root's home, so the caller's home is passed on for `~`.
fn rerun_as_root(program: &str, home: Option<&Path>) -> std::io::Error {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => return err,
    };
    let mut process = Process::new(program);
    process.arg(exe);
    // Always passed, empty without a home, so the rerun never looks one up.
    let mut flag = OsString::from("--home=");
    flag.push(home.unwrap_or(Path::new("")));
    process.arg(flag);
    process.args(without_sudo(std::env::args_os().skip(1)));
    process.exec()
}

/// Drops `--sudo` and `--home`, which [`rerun_as_root`] replaces, from the
/// options before the subcommand, leaving the subcommand's arguments alone.
/// Every option before the subcommand takes `=` for a value, so the first
/// argument without a leading `-` is the subcommand.
fn without_sudo(args: impl Iterator<Item = OsString>) -> Vec<OsString> {
    let mut in_options = true;
    args.filter(|arg| {
        let bytes = arg.as_encoded_bytes();
        in_options &= bytes.starts_with(b"-");
        let replaced =
            bytes == b"--sudo" || bytes.starts_with(b"--sudo=") || bytes.starts_with(b"--home=");
        !(in_options && replaced)
    })
    .collect()
}

/// Whether to ask before running `query`: a recursive update as root can
/// change a whole tree of other users' files.
fn needs_confirmation(query: &FileQuery, yes: bool, is_root: bool) -> bool {
    matches!(
        query,
        FileQuery::Update {
            recursive: true,
            ..
        }
    ) && is_root
        && !yes
}

/// Asks `question` on `output` and reads a yes or no from `input`. Without a
/// terminal to answer on, it refuses rather than read an answer from a pipe.
fn confirm(
    question: &str,
    input: &mut impl BufRead,
    interactive: bool,
    output: &mut impl Write,
) -> Result<bool, Box<dyn Error>> {
    if !interactive {
        return Err(format!("{question} Pass --yes to confirm without a terminal").into());
    }
    write!(output, "{question} [y/N] ")?;
    output.flush()?;
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

/// Run the application in SQL mode
fn run_sql_mode(args: &SqlCommand, home: Option<&Path>) -> Result<(), Box<dyn Error>> {
    let query = parse_sql_with_home(&args.query, home).map_err(FMQLError::from)?;
    if let FileQuery::Group { .. } = query {
        let table = execute_grouped(&query).map_err(FMQLError::from)?;
        match args.format {
            Format::Json => println!("{}", serde_json::to_string_pretty(&table_json(&table))?),
            Format::Text => print!("{}", render_table(&table)),
        }
        return Ok(());
    }
    let results = if args.dry_run && matches!(query, FileQuery::Update { .. }) {
        let planned = plan_update(&query).map_err(FMQLError::from)?;
        eprintln!("Dry run: nothing was changed");
        planned
    } else if needs_confirmation(&query, args.yes, nix::unistd::geteuid().is_root()) {
        let FileQuery::Update { path, .. } = &query else {
            unreachable!("only updates need confirmation")
        };
        let planned = plan_update(&query).map_err(FMQLError::from)?;
        if planned.is_empty() {
            // Nothing to ask about, so nothing runs unasked either.
            planned
        } else {
            let count = planned.len();
            let entries = if count == 1 { "entry" } else { "entries" };
            let question = format!(
                "Update {count} matching {entries} under {} as root?",
                path.display()
            );
            let stdin = std::io::stdin();
            let interactive = stdin.is_terminal();
            if !confirm(
                &question,
                &mut stdin.lock(),
                interactive,
                &mut std::io::stderr(),
            )? {
                return Err("Cancelled: nothing was changed".into());
            }
            // The count is a preview: like any update, this reads the tree
            // again and changes whatever matches now.
            execute_query(&query).map_err(FMQLError::from)?
        }
    } else {
        execute_query(&query).map_err(FMQLError::from)?
    };
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

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
