//! Rendering query results for the terminal.

use std::fmt::Write;

use crate::sql::executor::FileResult;

/// Renders results as one `ls -l` style line per entry: type and permission
/// bits, size in bytes, modification time in UTC, and path.
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
        let _ = writeln!(
            out,
            "{}  {:>size_width$}  {}  {}",
            mode_string(file.permissions, file.is_directory),
            file.size,
            file.modified.format("%Y-%m-%d %H:%M"),
            file.path.display(),
        );
    }
    out
}

/// Formats a mode as `ls` does, for example `drwxr-x---` or `-rwsr-xr-x`.
#[must_use]
pub fn mode_string(mode: u32, is_directory: bool) -> String {
    let mut out = String::with_capacity(10);
    out.push(if is_directory { 'd' } else { '-' });
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
    fn special_bits_replace_execute() {
        assert_eq!(mode_string(0o4755, false), "-rwsr-xr-x");
        assert_eq!(mode_string(0o2644, false), "-rw-r-Sr--");
        assert_eq!(mode_string(0o1777, true), "drwxrwxrwt");
        assert_eq!(mode_string(0o1666, true), "drw-rw-rwT");
    }

    #[test]
    fn empty_results_render_nothing() {
        assert_eq!(super::render_text(&[]), "");
    }
}
