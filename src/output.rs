//! Rendering query results for the terminal.

use std::fmt::Write;

use serde_json::{Map, Value};

use crate::sql::ast::FileAttribute;
use crate::sql::executor::FileResult;

/// Renders results as one `ls -l` style line per entry: type and permission
/// bits, size in bytes, modification time in UTC, and path. A symbolic link
/// shows `l` as its type, with its target's permissions and size.
///
/// # Examples
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use fmql::output::render_text;
/// use fmql::sql::executor::FileResult;
///
/// let file = FileResult {
///     path: "notes/todo.txt".into(),
///     name: "todo.txt".into(),
///     size: 42,
///     is_directory: false,
///     extension: Some("txt".into()),
///     permissions: 0o100644,
///     modified: Utc.with_ymd_and_hms(2025, 3, 1, 9, 30, 0).unwrap(),
///     created: None,
///     accessed: None,
///     is_symlink: false,
///     owner: None,
/// };
/// assert_eq!(
///     render_text(&[file]),
///     "-rw-r--r--  42  2025-03-01 09:30  notes/todo.txt\n"
/// );
/// ```
#[must_use]
pub fn render_text(results: &[FileResult]) -> String {
    let size_width = results
        .iter()
        .map(|file| file.size.to_string().len())
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for file in results {
        let mode = ls_mode(file);
        let _ = writeln!(
            out,
            "{mode}  {:>size_width$}  {}  {}",
            file.size,
            file.modified.format("%Y-%m-%d %H:%M"),
            file.path.display(),
        );
    }
    out
}

/// Renders the selected attributes of each entry as aligned columns separated
/// by two spaces. Sizes are right-aligned, permissions are octal, times are UTC
/// and unknown values are `-`.
///
/// # Examples
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use fmql::output::render_columns;
/// use fmql::sql::ast::FileAttribute;
/// use fmql::sql::executor::FileResult;
///
/// let file = |name: &str, size| FileResult {
///     path: name.into(),
///     name: name.into(),
///     size,
///     is_directory: false,
///     extension: None,
///     permissions: 0o100640,
///     modified: Utc.with_ymd_and_hms(2025, 3, 1, 9, 30, 0).unwrap(),
///     created: None,
///     accessed: None,
///     is_symlink: false,
///     owner: None,
/// };
/// let columns = [FileAttribute::Name, FileAttribute::Size, FileAttribute::Owner];
/// assert_eq!(
///     render_columns(&[file("a", 1200), file("bcd", 7)], &columns),
///     "a    1200  -\nbcd     7  -\n"
/// );
/// ```
#[must_use]
pub fn render_columns(results: &[FileResult], attributes: &[FileAttribute]) -> String {
    let rows: Vec<Vec<String>> = results
        .iter()
        .map(|file| attributes.iter().map(|a| cell(file, a)).collect())
        .collect();
    let widths: Vec<usize> = (0..attributes.len())
        .map(|i| {
            rows.iter()
                .map(|row| row[i].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let mut out = String::new();
    for row in &rows {
        let mut line = String::new();
        for (i, (value, attribute)) in row.iter().zip(attributes).enumerate() {
            if i > 0 {
                line.push_str("  ");
            }
            let width = widths[i];
            if *attribute == FileAttribute::Size {
                let _ = write!(line, "{value:>width$}");
            } else if i + 1 == row.len() {
                // No padding after the last column. Trimming the line instead
                // would also strip spaces that belong to a file name.
                line.push_str(value);
            } else {
                let _ = write!(line, "{value:<width$}");
            }
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// Converts each entry to a JSON object holding only the selected attributes,
/// keyed by attribute name. Values match the full JSON output: times are Unix
/// seconds, `permissions` is the raw mode, and unknown values are `null`.
#[must_use]
pub fn columns_json(results: &[FileResult], attributes: &[FileAttribute]) -> Value {
    results
        .iter()
        .map(|file| {
            attributes
                .iter()
                .map(|a| (a.name().to_owned(), json_value(file, a)))
                .collect::<Map<_, _>>()
        })
        .collect()
}

fn cell(file: &FileResult, attribute: &FileAttribute) -> String {
    let time = |t: Option<chrono::DateTime<chrono::Utc>>| {
        t.map_or_else(
            || "-".to_owned(),
            |t| t.format("%Y-%m-%d %H:%M").to_string(),
        )
    };
    match attribute {
        FileAttribute::Name => file.name.clone(),
        FileAttribute::Path => file.path.display().to_string(),
        FileAttribute::Size => file.size.to_string(),
        FileAttribute::Extension => file.extension.clone().unwrap_or_else(|| "-".to_owned()),
        FileAttribute::Owner => file.owner.clone().unwrap_or_else(|| "-".to_owned()),
        FileAttribute::Permissions => format!("{:o}", file.permissions & 0o7777),
        FileAttribute::Modified => time(Some(file.modified)),
        FileAttribute::Created => time(file.created),
        FileAttribute::Accessed => time(file.accessed),
        FileAttribute::IsDirectory => file.is_directory.to_string(),
        FileAttribute::IsSymlink => file.is_symlink.to_string(),
        FileAttribute::Mode => ls_mode(file),
        FileAttribute::IsExecutable => (file.permissions & 0o100 != 0).to_string(),
        FileAttribute::All => render_text(std::slice::from_ref(file))
            .trim_end()
            .to_owned(),
    }
}

fn json_value(file: &FileResult, attribute: &FileAttribute) -> Value {
    let time = |t: Option<chrono::DateTime<chrono::Utc>>| t.map(|t| t.timestamp()).into();
    match attribute {
        FileAttribute::Name => file.name.clone().into(),
        FileAttribute::Path => file.path.to_string_lossy().into_owned().into(),
        FileAttribute::Size => file.size.into(),
        FileAttribute::Extension => file.extension.clone().into(),
        FileAttribute::Owner => file.owner.clone().into(),
        FileAttribute::Permissions => file.permissions.into(),
        FileAttribute::Modified => time(Some(file.modified)),
        FileAttribute::Created => time(file.created),
        FileAttribute::Accessed => time(file.accessed),
        FileAttribute::IsDirectory => file.is_directory.into(),
        FileAttribute::IsSymlink => file.is_symlink.into(),
        FileAttribute::Mode => ls_mode(file).into(),
        FileAttribute::IsExecutable => (file.permissions & 0o100 != 0).into(),
        FileAttribute::All => serde_json::to_value(file).unwrap_or(Value::Null),
    }
}

/// The entry's type and permissions as `ls -l` shows them. A symbolic link
/// shows `l` as its type, with its target's permissions.
#[must_use]
pub fn ls_mode(file: &FileResult) -> String {
    let mut mode = mode_string(file.permissions, file.is_directory);
    if file.is_symlink {
        mode.replace_range(..1, "l");
    }
    mode
}

/// Formats a mode as `ls` does, for example `drwxr-x---` or `-rwsr-xr-x`.
///
/// The type letter comes from the mode's file-type bits: `d`, `l`, `p` (FIFO),
/// `s` (socket), `c` and `b` (character and block devices) or `-`. A mode
/// holding only permission bits falls back to `is_directory`.
#[must_use]
pub fn mode_string(mode: u32, is_directory: bool) -> String {
    let mut out = String::with_capacity(10);
    out.push(match mode & 0o170_000 {
        0o040_000 => 'd',
        0o120_000 => 'l',
        0o010_000 => 'p',
        0o140_000 => 's',
        0o020_000 => 'c',
        0o060_000 => 'b',
        0o100_000 => '-',
        _ if is_directory => 'd',
        _ => '-',
    });
    // Each class has its read, write and execute bits, plus the special bit
    // (setuid, setgid, sticky) shown in the execute position.
    for (shift, special, set_char) in [(6, 0o4000, 's'), (3, 0o2000, 's'), (0, 0o1000, 't')] {
        let bits = (mode >> shift) & 0o7;
        out.push(if bits & 0o4 == 0 { '-' } else { 'r' });
        out.push(if bits & 0o2 == 0 { '-' } else { 'w' });
        out.push(match (bits & 0o1 != 0, mode & special != 0) {
            (true, true) => set_char,
            (false, true) => set_char.to_ascii_uppercase(),
            (true, false) => 'x',
            (false, false) => '-',
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::mode_string;

    #[test]
    fn plain_modes() {
        assert_eq!(mode_string(0o644, false), "-rw-r--r--");
        assert_eq!(mode_string(0o40755, true), "drwxr-xr-x");
        assert_eq!(mode_string(0, false), "----------");
    }

    #[test]
    fn type_bits_choose_the_type_letter() {
        assert_eq!(mode_string(0o100_644, true), "-rw-r--r--");
        assert_eq!(mode_string(0o010_644, false), "prw-r--r--");
        assert_eq!(mode_string(0o140_755, false), "srwxr-xr-x");
        assert_eq!(mode_string(0o020_620, false), "crw--w----");
        assert_eq!(mode_string(0o060_660, false), "brw-rw----");
        assert_eq!(mode_string(0o120_777, false), "lrwxrwxrwx");
    }

    #[test]
    fn special_bits_replace_execute() {
        assert_eq!(mode_string(0o4755, false), "-rwsr-xr-x");
        assert_eq!(mode_string(0o2644, false), "-rw-r-Sr--");
        assert_eq!(mode_string(0o1777, true), "drwxrwxrwt");
        assert_eq!(mode_string(0o1666, true), "drw-rw-rwT");
    }

    #[test]
    fn last_column_keeps_trailing_spaces_in_names() {
        use crate::sql::ast::FileAttribute;
        use crate::sql::executor::FileResult;
        use chrono::Utc;

        let file = |name: &str| FileResult {
            path: name.into(),
            name: name.into(),
            size: 1,
            is_directory: false,
            extension: None,
            permissions: 0o100_644,
            modified: Utc::now(),
            created: None,
            accessed: None,
            is_symlink: false,
            owner: None,
        };
        let rendered = super::render_columns(
            &[file("long-name"), file("report ")],
            &[FileAttribute::Size, FileAttribute::Name],
        );
        assert_eq!(rendered, "1  long-name\n1  report \n");
    }

    #[test]
    fn empty_results_render_nothing() {
        assert_eq!(super::render_text(&[]), "");
    }
}
