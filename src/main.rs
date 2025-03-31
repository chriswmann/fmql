//! ls-rs - A fast and feature-rich file manager written in Rust.
//! 
//! This crate provides a command-line tool for listing and managing files with various
//! features like sorting, grouping, and detailed file information.
//! 
//! # Examples
//! 
//! ```rust
//! use ls_rs::cli::Args;
//! use ls_rs::file::list_directory;
//! 
//! let args = Args::parse();
//! let files = list_directory(&args).unwrap();
//! 
//! for file in files {
//!     println!("{}", file.name);
//! }
//! ```

mod cli;
mod display;
mod file;
mod error;

use clap::Parser;
use cli::Args;
use display::display_file_list;
use file::list_directory;
use error::FileManagerError;

/// Run the file manager with the given arguments.
/// 
/// # Arguments
/// 
/// * `args` - The command line arguments parsed into an `Args` struct.
/// 
/// # Returns
/// 
/// * `Result<(), FileManagerError>` - Returns `Ok(())` on success, or an error on failure.
/// 
/// # Examples
/// 
/// ```no_run
/// use file_manager::cli::Args;
/// 
/// let args = Args::parse();
/// file_manager::run(args)?;
/// ```
pub fn run(args: Args) -> Result<(), FileManagerError> {
    let files = list_directory(&args)?;
    display_file_list(&files, &args);
    Ok(())
}

fn main() {
    let args = Args::parse();

    if let Err(e) = args.validate() {
        eprintln!("Error validating arguments: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = run(args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
} 