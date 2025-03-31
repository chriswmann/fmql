use crate::file::list_directory;
use crate::display::display_file_list;
use crate::cli::Args;

pub fn run(args: Args) -> std::io::Result<()> {
    let files = list_directory(&args)?;
    display_file_list(&files, &args);
    Ok(())
} 