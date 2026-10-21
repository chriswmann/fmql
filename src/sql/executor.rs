//! Executor for SQL-like file management commands.
//!
//! This module provides functionality to execute parsed SQL-like commands
//! on the file system, such as querying files or updating file attributes.

use chrono::{DateTime, Utc};
use nix::unistd::{Uid, User};
use regex::{Regex, RegexBuilder};
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::{self, Permissions};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::WalkDir;

use crate::sql::ast::{
    ComparisonOperator, FileAttribute, FileAttributeUpdate, FileCondition, FileQuery, FileValue,
    NameExpr, OrderBy, UpdateValue,
};

/// Errors that can occur during query execution.
#[derive(Error, Debug)]
pub enum ExecutorError {
    /// Error from `std::io` operations.
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Error when a file attribute is not supported.
    #[error("Unsupported file attribute: {0}")]
    UnsupportedAttribute(String),

    /// Error when an operation is not supported.
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    /// Error when a regular expression is invalid.
    #[error("Invalid regular expression: {0}")]
    InvalidRegex(#[from] regex::Error),

    /// Error when a value is of the wrong type.
    #[error("Type error: {0}")]
    TypeError(String),

    /// An owner named in an update is not in the user database.
    #[error("Unknown user: {0}")]
    UnknownUser(String),

    /// A computed new name is not usable.
    #[error("Cannot rename {} to '{name}': {reason}", path.display())]
    InvalidName {
        /// The file being renamed.
        path: PathBuf,
        /// The computed name.
        name: String,
        /// Why it was rejected.
        reason: String,
    },

    /// A rename would overwrite another entry or share a destination.
    #[error(
        "Cannot rename {} to {}: it already exists or another rename uses it",
        path.display(),
        target.display()
    )]
    RenameConflict {
        /// The file being renamed.
        path: PathBuf,
        /// The destination.
        target: PathBuf,
    },

    /// An update failed part way; every earlier change was undone.
    #[error("Failed to update {}: {source}; earlier changes were rolled back", path.display())]
    UpdateFailed {
        /// The file that could not be changed.
        path: PathBuf,
        /// Why the change failed.
        source: std::io::Error,
    },

    /// An update failed part way and some earlier changes, to owner or
    /// permissions, could not be undone.
    #[error(
        "Failed to update {}: {source}; could not undo the changes to {}",
        path.display(),
        display_paths(unrestored)
    )]
    RollbackFailed {
        /// The file that could not be changed.
        path: PathBuf,
        /// Why the change failed.
        source: std::io::Error,
        /// Files left with the new permissions.
        unrestored: Vec<PathBuf>,
    },
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Result type for executor operations.
pub type Result<T> = std::result::Result<T, ExecutorError>;

/// Represents a file that matches a query.
#[derive(Debug, Clone, Serialize)]
pub struct FileResult {
    /// The file path.
    pub path: PathBuf,
    /// The file name.
    pub name: String,
    /// The file size in bytes.
    pub size: u64,
    /// Whether the file is a directory.
    pub is_directory: bool,
    /// The file extension, if any.
    pub extension: Option<String>,
    /// The file permissions.
    pub permissions: u32,
    /// The file modification time.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub modified: DateTime<Utc>,
    /// The file creation time, if the filesystem records one.
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub created: Option<DateTime<Utc>>,
    /// The file access time, if available.
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub accessed: Option<DateTime<Utc>>,
    /// Whether the entry is a symbolic link. Other fields describe the link's
    /// target, or the link itself when the target is missing.
    pub is_symlink: bool,
    /// The name of the file's owner, if the user database knows its uid.
    pub owner: Option<String>,
}

/// Executes a parsed `FileQuery`.
///
/// # Arguments
///
/// * `query` - The parsed `FileQuery` to execute.
///
/// # Returns
///
/// A Result containing a vector of `FileResult` instances or an `ExecutorError`.
///
/// # Errors
///
/// Returns an error if the directory cannot be read, a file's metadata cannot
/// be read, or an update requests an unsupported attribute or fails to apply.
///
/// # Examples
///
/// Example of using `execute_query` to process a parsed query:
///
/// ```no_run
/// use fmql::sql::{parse_sql, execute_query};
///
/// // Parse a query
/// let query = parse_sql("SELECT * FROM /var/log WHERE name LIKE '%.log'").unwrap();
///
/// // Execute the query to get matching files
/// let results = execute_query(&query).unwrap();
///
/// // Process the results
/// for file in results {
///     println!("{}: {} bytes", file.name, file.size);
/// }
/// ```
pub fn execute_query(query: &FileQuery) -> Result<Vec<FileResult>> {
    match query {
        FileQuery::Select {
            path,
            recursive,
            attributes,
            condition,
            order_by,
            limit,
        } => execute_select(
            path,
            *recursive,
            attributes,
            condition.as_ref(),
            order_by,
            *limit,
        ),
        FileQuery::Update {
            path,
            recursive,
            updates,
            condition,
        } => execute_update(path, *recursive, updates, condition.as_ref()),
    }
}

/// Executes a SELECT query.
fn execute_select(
    path: &Path,
    recursive: bool,
    _attributes: &[FileAttribute],
    condition: Option<&FileCondition>,
    order_by: &[OrderBy],
    limit: Option<usize>,
) -> Result<Vec<FileResult>> {
    for key in order_by {
        attribute_kind(&key.attribute)?;
    }
    let predicate = condition.map(Predicate::compile).transpose()?;
    let mut files = matching_files(path, recursive, predicate.as_ref())?;
    if !order_by.is_empty() {
        // A stable sort keeps traversal order between equal keys.
        files.sort_by(|a, b| {
            order_by
                .iter()
                .map(|key| {
                    let ordering = order_values(
                        &attribute_value(a, &key.attribute),
                        &attribute_value(b, &key.attribute),
                    );
                    if key.descending {
                        ordering.reverse()
                    } else {
                        ordering
                    }
                })
                .find(|ordering| ordering.is_ne())
                .unwrap_or(Ordering::Equal)
        });
    }
    if let Some(limit) = limit {
        files.truncate(limit);
    }
    Ok(files)
}

/// Executes an UPDATE query.
///
/// Every update and the condition are validated, and every entry is evaluated,
/// before any file is changed.
fn execute_update(
    path: &Path,
    recursive: bool,
    updates: &[FileAttributeUpdate],
    condition: Option<&FileCondition>,
) -> Result<Vec<FileResult>> {
    update_with(path, recursive, updates, condition, apply_op)
}

/// [`execute_update`] with the filesystem operation supplied, so tests can
/// make a change fail part way through a real query.
fn update_with(
    path: &Path,
    recursive: bool,
    updates: &[FileAttributeUpdate],
    condition: Option<&FileCondition>,
    apply: impl FnMut(&Path, &Op) -> std::io::Result<()>,
) -> Result<Vec<FileResult>> {
    let changes = Changes::plan(updates)?;
    let predicate = condition.map(Predicate::compile).transpose()?;
    if changes.is_empty() {
        return Ok(Vec::new());
    }

    let mut targets: Vec<FileResult> = matching_files(path, recursive, predicate.as_ref())?
        .into_iter()
        // Changing permissions or ownership acts on a link's target, which may
        // lie outside the queried directory, so links are never updated.
        .filter(|file| !file.is_symlink)
        .collect();
    let renames = changes.renames(&targets)?;
    let paths: Vec<PathBuf> = targets.iter().map(|file| file.path.clone()).collect();
    apply_changes(&paths, &changes, &renames, apply)?;

    // Renames ran deepest first, so replaying them in order also moves the
    // reported paths of entries inside renamed directories.
    for (from, to) in &renames {
        for file in &mut targets {
            let Ok(rest) = file.path.strip_prefix(from).map(Path::to_path_buf) else {
                continue;
            };
            if rest.as_os_str().is_empty() {
                // `join("")` would add a trailing separator.
                file.path.clone_from(to);
                file.name = file
                    .path
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                file.extension = file
                    .path
                    .extension()
                    .map(|ext| ext.to_string_lossy().into_owned());
            } else {
                file.path = to.join(rest);
            }
        }
    }
    // Update the listed results rather than re-reading metadata, so a
    // successful update cannot then fail. This follows the renames so that
    // the one metadata read below uses each file's new path.
    for file in &mut targets {
        if let Some((_, name)) = &changes.owner {
            file.owner = Some(name.clone());
            // A change of owner can clear the setuid and setgid bits.
            if let Ok(metadata) = fs::metadata(&file.path) {
                file.permissions = metadata.permissions().mode();
            }
        }
        if let Some(mode) = changes.mode {
            file.permissions = (file.permissions & !0o7777) | mode;
        }
    }
    Ok(targets)
}

/// One filesystem change made by an update.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Op {
    /// Set the permission bits.
    SetMode(u32),
    /// Set the owning uid, leaving the group alone.
    SetOwner(u32),
    /// Move the file to this path, in the same directory.
    Rename(PathBuf),
}

/// Applies an [`Op`] to the real filesystem.
fn apply_op(path: &Path, op: &Op) -> std::io::Result<()> {
    match op {
        Op::SetMode(mode) => fs::set_permissions(path, Permissions::from_mode(*mode)),
        Op::SetOwner(uid) => std::os::unix::fs::chown(path, Some(*uid), None),
        Op::Rename(to) => rename_no_replace(path, to),
    }
}

/// Renames `from` to `to`, failing with `AlreadyExists` rather than replacing
/// an entry that appeared after the update was planned, such as one created
/// by another process or a case-only collision on a case-insensitive
/// filesystem. The kernel enforces this where it can.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use nix::errno::Errno;
    use nix::fcntl::{AT_FDCWD, RenameFlags, renameat2};

    match renameat2(AT_FDCWD, from, AT_FDCWD, to, RenameFlags::RENAME_NOREPLACE) {
        Ok(()) => Ok(()),
        // A case-only rename of the same file on a case-insensitive filesystem.
        Err(Errno::EEXIST) if !occupied(from, to) => fs::rename(from, to),
        // The filesystem cannot refuse to replace, so check first instead.
        Err(Errno::EINVAL | Errno::ENOSYS) => checked_rename(from, to),
        Err(errno) => Err(errno.into()),
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    checked_rename(from, to)
}

/// Renames unless `to` names another entry. Another process can still create
/// `to` between the check and the rename; this only narrows that window.
fn checked_rename(from: &Path, to: &Path) -> std::io::Result<()> {
    if occupied(from, to) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} already exists", to.display()),
        ));
    }
    fs::rename(from, to)
}

/// The validated effect of an update's assignments. Later assignments to the
/// same attribute win.
#[derive(Debug, Default)]
struct Changes {
    mode: Option<u32>,
    owner: Option<(u32, String)>,
    name: Option<NameExpr>,
}

impl Changes {
    fn plan(updates: &[FileAttributeUpdate]) -> Result<Self> {
        let mut changes = Self::default();
        for update in updates {
            match (&update.attribute, &update.value) {
                (FileAttribute::Permissions, UpdateValue::Literal(value)) => {
                    changes.mode = Some(permission_mode(value)?);
                }
                (FileAttribute::Owner, UpdateValue::Literal(value)) => {
                    let user = User::from_name(value)
                        .ok()
                        .flatten()
                        .ok_or_else(|| ExecutorError::UnknownUser(value.clone()))?;
                    changes.owner = Some((user.uid.as_raw(), user.name));
                }
                (FileAttribute::Name, UpdateValue::Name(expr)) => {
                    check_name_expr(expr)?;
                    changes.name = Some(expr.clone());
                }
                (attribute, value) => {
                    return Err(ExecutorError::UnsupportedAttribute(format!(
                        "Cannot update attribute {attribute:?} with {value:?}"
                    )));
                }
            }
        }
        Ok(changes)
    }

    fn is_empty(&self) -> bool {
        self.mode.is_none() && self.owner.is_none() && self.name.is_none()
    }

    /// The attribute operations for one file. Ownership goes first because
    /// `chown` can clear the setuid and setgid bits that a new mode might set.
    fn ops(&self) -> impl Iterator<Item = Op> {
        self.owner
            .iter()
            .map(|(uid, _)| Op::SetOwner(*uid))
            .chain(self.mode.map(Op::SetMode))
    }

    /// Computes and checks each target's new path, deepest first so a
    /// directory is renamed only after the entries inside it. Unchanged names
    /// are left out. Fails, before anything changes, on an invalid name or a
    /// destination that exists or is shared.
    fn renames(&self, targets: &[FileResult]) -> Result<Vec<(PathBuf, PathBuf)>> {
        let Some(expr) = &self.name else {
            return Ok(Vec::new());
        };
        let mut destinations = std::collections::HashSet::new();
        let mut renames = Vec::new();
        for file in targets {
            let Some(name) = file.path.file_name().and_then(|n| n.to_str()) else {
                return Err(invalid_name(
                    &file.path,
                    &file.name,
                    "it is not valid UTF-8",
                ));
            };
            let new_name = eval_name(expr, name, file.extension.as_deref());
            if new_name == name {
                continue;
            }
            if new_name.is_empty() || new_name == "." || new_name == ".." {
                return Err(invalid_name(&file.path, &new_name, "it is not a file name"));
            }
            if new_name.contains(['/', '\0']) {
                return Err(invalid_name(
                    &file.path,
                    &new_name,
                    "it contains '/' or NUL",
                ));
            }
            let to = file.path.with_file_name(&new_name);
            if !destinations.insert(to.clone()) || occupied(&file.path, &to) {
                return Err(ExecutorError::RenameConflict {
                    path: file.path.clone(),
                    target: to,
                });
            }
            renames.push((file.path.clone(), to));
        }
        renames.sort_by_key(|(from, _)| std::cmp::Reverse(from.components().count()));
        Ok(renames)
    }
}

/// Whether `to` names an existing entry other than `from` itself. A rename
/// that only changes case on a case-insensitive filesystem finds `from`.
fn occupied(from: &Path, to: &Path) -> bool {
    match (fs::symlink_metadata(from), fs::symlink_metadata(to)) {
        (_, Err(_)) => false,
        (Ok(a), Ok(b)) => (a.dev(), a.ino()) != (b.dev(), b.ino()),
        (Err(_), Ok(_)) => true,
    }
}

fn invalid_name(path: &Path, name: &str, reason: &str) -> ExecutorError {
    ExecutorError::InvalidName {
        path: path.to_path_buf(),
        name: name.to_owned(),
        reason: reason.to_owned(),
    }
}

/// Rejects name expressions the parser would not produce: attributes other
/// than `name` and `extension`, and empty `REPLACE` patterns.
fn check_name_expr(expr: &NameExpr) -> Result<()> {
    match expr {
        NameExpr::Literal(_)
        | NameExpr::Attribute(FileAttribute::Name | FileAttribute::Extension) => Ok(()),
        NameExpr::Attribute(attribute) => Err(ExecutorError::UnsupportedAttribute(format!(
            "Attribute not supported in a new name: {attribute:?}"
        ))),
        NameExpr::Concat(parts) => parts.iter().try_for_each(check_name_expr),
        NameExpr::Replace { from, .. } if from.is_empty() => Err(ExecutorError::TypeError(
            "REPLACE needs a non-empty string to find".to_string(),
        )),
        NameExpr::Replace { expr, .. } | NameExpr::Lower(expr) | NameExpr::Upper(expr) => {
            check_name_expr(expr)
        }
    }
}

/// Computes a new name for a file called `name`.
fn eval_name(expr: &NameExpr, name: &str, extension: Option<&str>) -> String {
    match expr {
        NameExpr::Literal(text) => text.clone(),
        NameExpr::Attribute(FileAttribute::Extension) => extension.unwrap_or_default().to_owned(),
        NameExpr::Attribute(_) => name.to_owned(),
        NameExpr::Concat(parts) => parts
            .iter()
            .map(|part| eval_name(part, name, extension))
            .collect(),
        NameExpr::Replace { expr, from, to } => eval_name(expr, name, extension).replace(from, to),
        NameExpr::Lower(expr) => eval_name(expr, name, extension).to_lowercase(),
        NameExpr::Upper(expr) => eval_name(expr, name, extension).to_uppercase(),
    }
}

/// A change an update made, recorded so it can be undone.
enum Undo<'a> {
    /// A file's owner and permission bits before its attributes changed.
    Attributes { path: &'a Path, uid: u32, mode: u32 },
    /// A completed rename.
    Renamed { from: &'a Path, to: &'a Path },
}

/// Applies `changes` to every path, then the renames, or applies none of
/// them: if a change fails, everything done so far, including a partly
/// changed file, is undone newest first.
fn apply_changes(
    paths: &[PathBuf],
    changes: &Changes,
    renames: &[(PathBuf, PathBuf)],
    mut apply: impl FnMut(&Path, &Op) -> std::io::Result<()>,
) -> Result<()> {
    let mut log: Vec<Undo<'_>> = Vec::new();
    let attribute_ops: Vec<Op> = changes.ops().collect();
    let mut outcome = Ok(());
    if !attribute_ops.is_empty() {
        for path in paths {
            outcome = fs::metadata(path)
                .and_then(|metadata| {
                    log.push(Undo::Attributes {
                        path,
                        uid: metadata.uid(),
                        mode: metadata.permissions().mode() & 0o7777,
                    });
                    attribute_ops.iter().try_for_each(|op| apply(path, op))
                })
                .map_err(|source| (path, source));
            if outcome.is_err() {
                break;
            }
        }
    }
    if outcome.is_ok() {
        for (from, to) in renames {
            outcome = apply(from, &Op::Rename(to.clone())).map_err(|source| (from, source));
            if outcome.is_err() {
                break;
            }
            log.push(Undo::Renamed { from, to });
        }
    }
    let Err((path, source)) = outcome else {
        return Ok(());
    };
    let unrestored: Vec<PathBuf> = log
        .iter()
        .rev()
        .filter_map(|undo| undo_one(undo, changes, &mut apply).err())
        .collect();
    let path = path.clone();
    Err(if unrestored.is_empty() {
        ExecutorError::UpdateFailed { path, source }
    } else {
        ExecutorError::RollbackFailed {
            path,
            source,
            unrestored,
        }
    })
}

/// Undoes one change, returning where the file now is if that fails. An
/// owner is restored before the mode, which restoring the owner may clear.
fn undo_one(
    undo: &Undo<'_>,
    changes: &Changes,
    apply: &mut impl FnMut(&Path, &Op) -> std::io::Result<()>,
) -> std::result::Result<(), PathBuf> {
    let result = match undo {
        Undo::Attributes { path, uid, mode } => {
            let owner = if changes.owner.is_some() {
                apply(path, &Op::SetOwner(*uid))
            } else {
                Ok(())
            };
            owner.and_then(|()| apply(path, &Op::SetMode(*mode)))
        }
        Undo::Renamed { from, to } => apply(to, &Op::Rename(from.to_path_buf())),
    };
    result.map_err(|_| match undo {
        Undo::Attributes { path, .. } => path.to_path_buf(),
        Undo::Renamed { to, .. } => to.to_path_buf(),
    })
}

/// Reads an octal mode from `0` to `7777`.
fn permission_mode(value: &str) -> Result<u32> {
    u32::from_str_radix(value, 8)
        .ok()
        .filter(|mode| *mode <= 0o7777)
        .ok_or_else(|| ExecutorError::TypeError(format!("Invalid permissions value: {value}")))
}

/// Lists the entries under `path` that satisfy `predicate`.
///
/// Callers compile the predicate before calling, so an invalid condition fails
/// without touching the filesystem.
fn matching_files(
    path: &Path,
    recursive: bool,
    predicate: Option<&Predicate>,
) -> Result<Vec<FileResult>> {
    let files = list_files(path, recursive)?;
    Ok(match predicate {
        Some(predicate) => files
            .into_iter()
            .filter(|file| predicate.evaluate(file))
            .collect(),
        None => files,
    })
}

/// Lists files in a directory, optionally recursively.
fn list_files(dir_path: &Path, recursive: bool) -> Result<Vec<FileResult>> {
    let mut results = Vec::new();
    let mut owners = Owners::default();

    let walker = if recursive {
        WalkDir::new(dir_path)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
    } else {
        WalkDir::new(dir_path)
            .max_depth(1)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
    };

    for entry in walker {
        let entry = entry.map_err(|e| {
            ExecutorError::IoError(std::io::Error::other(format!(
                "Failed to read directory entry: {e}"
            )))
        })?;

        // Skip the root directory itself -- we only want its children.
        if entry.path() == dir_path && entry.file_type().is_dir() {
            continue;
        }

        let file_result = create_file_result(entry.path(), &mut owners)?;
        results.push(file_result);
    }

    Ok(results)
}

/// Looks up owner names, remembering each uid so a directory of files with
/// the same owner reads the user database once.
#[derive(Default)]
struct Owners(HashMap<u32, Option<String>>);

impl Owners {
    fn name(&mut self, uid: u32) -> Option<String> {
        self.0
            .entry(uid)
            .or_insert_with(|| {
                User::from_uid(Uid::from_raw(uid))
                    .ok()
                    .flatten()
                    .map(|user| user.name)
            })
            .clone()
    }
}

/// Creates a `FileResult` from a path.
fn create_file_result(path: &Path, owners: &mut Owners) -> Result<FileResult> {
    let link = fs::symlink_metadata(path)?;
    let is_symlink = link.file_type().is_symlink();
    // Describe a link by its target, falling back to the link itself when the
    // target is missing so one broken link cannot fail the whole query.
    let metadata = if is_symlink {
        fs::metadata(path).unwrap_or(link)
    } else {
        link
    };

    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());

    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().into_owned());

    let modified = metadata
        .modified()
        .map_or_else(|_| Utc::now(), DateTime::<Utc>::from);

    Ok(FileResult {
        path: path.to_path_buf(),
        name,
        size: metadata.len(),
        is_directory: metadata.is_dir(),
        extension,
        permissions: metadata.permissions().mode(),
        modified,
        created: timestamp(metadata.created()),
        accessed: timestamp(metadata.accessed()),
        is_symlink,
        owner: owners.name(metadata.uid()),
    })
}

/// Converts a timestamp the platform or filesystem may not provide.
fn timestamp(time: std::io::Result<std::time::SystemTime>) -> Option<DateTime<Utc>> {
    time.ok().map(DateTime::<Utc>::from)
}

/// The type of value an attribute produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Number,
    Time,
    Boolean,
}

/// Returns the kind of value `attribute` produces, or an error if its
/// metadata is not available.
fn attribute_kind(attribute: &FileAttribute) -> Result<Kind> {
    match attribute {
        FileAttribute::Name
        | FileAttribute::Path
        | FileAttribute::Extension
        | FileAttribute::Owner
        | FileAttribute::Mode => Ok(Kind::Text),
        FileAttribute::Size | FileAttribute::Permissions => Ok(Kind::Number),
        FileAttribute::Modified | FileAttribute::Created | FileAttribute::Accessed => {
            Ok(Kind::Time)
        }
        FileAttribute::IsDirectory | FileAttribute::IsExecutable | FileAttribute::IsSymlink => {
            Ok(Kind::Boolean)
        }
        FileAttribute::All => Err(ExecutorError::UnsupportedAttribute(
            "* is not an attribute that can be compared".to_string(),
        )),
    }
}

/// A condition whose attributes, operand types and patterns have been
/// checked, so evaluating it against a file cannot fail.
#[derive(Debug)]
enum Predicate {
    Compare {
        attribute: FileAttribute,
        operator: ComparisonOperator,
        value: FileValue,
    },
    Between {
        attribute: FileAttribute,
        lower: FileValue,
        upper: FileValue,
    },
    Matches {
        attribute: FileAttribute,
        regex: Regex,
    },
    And(Box<Predicate>, Box<Predicate>),
    Or(Box<Predicate>, Box<Predicate>),
    Not(Box<Predicate>),
}

impl Predicate {
    /// Checks a condition and compiles its patterns. Every branch is checked,
    /// including ones that short-circuiting might never evaluate.
    fn compile(condition: &FileCondition) -> Result<Self> {
        match condition {
            FileCondition::Compare {
                attribute,
                operator,
                value,
            } => {
                let kind = attribute_kind(attribute)?;
                let value = operand(attribute, kind, value)?;
                let ordering =
                    !matches!(operator, ComparisonOperator::Eq | ComparisonOperator::NotEq);
                if ordering && (kind == Kind::Boolean || matches!(value, FileValue::Null)) {
                    return Err(ExecutorError::UnsupportedOperation(format!(
                        "Operator {operator} not supported for {attribute:?} = {value:?}"
                    )));
                }
                Ok(Self::Compare {
                    attribute: attribute.clone(),
                    operator: operator.clone(),
                    value,
                })
            }
            FileCondition::Between {
                attribute,
                lower,
                upper,
            } => {
                let kind = attribute_kind(attribute)?;
                let lower = operand(attribute, kind, lower)?;
                let upper = operand(attribute, kind, upper)?;
                if kind == Kind::Boolean
                    || matches!(lower, FileValue::Null)
                    || matches!(upper, FileValue::Null)
                {
                    return Err(ExecutorError::UnsupportedOperation(format!(
                        "BETWEEN not supported for {attribute:?}"
                    )));
                }
                Ok(Self::Between {
                    attribute: attribute.clone(),
                    lower,
                    upper,
                })
            }
            FileCondition::Like {
                attribute,
                pattern,
                case_sensitive,
            } => {
                text_attribute(attribute, "LIKE")?;
                Ok(Self::Matches {
                    attribute: attribute.clone(),
                    regex: like_regex(pattern, *case_sensitive)?,
                })
            }
            FileCondition::Regexp { attribute, pattern } => {
                text_attribute(attribute, "REGEXP")?;
                Ok(Self::Matches {
                    attribute: attribute.clone(),
                    regex: Regex::new(pattern)?,
                })
            }
            FileCondition::And(left, right) => Ok(Self::And(
                Box::new(Self::compile(left)?),
                Box::new(Self::compile(right)?),
            )),
            FileCondition::Or(left, right) => Ok(Self::Or(
                Box::new(Self::compile(left)?),
                Box::new(Self::compile(right)?),
            )),
            FileCondition::Not(inner) => Ok(Self::Not(Box::new(Self::compile(inner)?))),
        }
    }

    /// Evaluates the predicate against a file.
    fn evaluate(&self, file: &FileResult) -> bool {
        match self {
            Self::Compare {
                attribute,
                operator,
                value,
            } => compare_values(&attribute_value(file, attribute), operator, value),
            Self::Between {
                attribute,
                lower,
                upper,
            } => {
                let file_value = attribute_value(file, attribute);
                compare_values(&file_value, &ComparisonOperator::GtEq, lower)
                    && compare_values(&file_value, &ComparisonOperator::LtEq, upper)
            }
            Self::Matches { attribute, regex } => match attribute_value(file, attribute) {
                FileValue::String(s) => regex.is_match(&s),
                // An unknown owner matches no pattern.
                _ => false,
            },
            Self::And(left, right) => left.evaluate(file) && right.evaluate(file),
            Self::Or(left, right) => left.evaluate(file) || right.evaluate(file),
            Self::Not(inner) => !inner.evaluate(file),
        }
    }
}

/// Rejects pattern matching on attributes that are not strings.
fn text_attribute(attribute: &FileAttribute, operation: &str) -> Result<()> {
    if attribute_kind(attribute)? == Kind::Text {
        Ok(())
    } else {
        Err(ExecutorError::TypeError(format!(
            "{operation} can only be used with string attributes, got {attribute:?}"
        )))
    }
}

/// Checks that a literal suits an attribute of the given kind, parsing date
/// strings so they compare as times.
fn operand(attribute: &FileAttribute, kind: Kind, value: &FileValue) -> Result<FileValue> {
    match (kind, value) {
        (_, FileValue::Null)
        | (Kind::Text, FileValue::String(_))
        | (Kind::Number, FileValue::Number(_))
        | (Kind::Time, FileValue::DateTime(_))
        | (Kind::Boolean, FileValue::Boolean(_)) => Ok(value.clone()),
        (Kind::Time, FileValue::String(s)) => parse_date_string(s).map(FileValue::DateTime),
        _ => Err(ExecutorError::TypeError(format!(
            "Cannot compare {attribute:?} with {value:?}"
        ))),
    }
}

/// Converts a SQL LIKE pattern, where `%` matches any run of characters and
/// `_` matches one, into an anchored regular expression. Everything else is
/// literal.
fn like_regex(pattern: &str, case_sensitive: bool) -> Result<Regex> {
    let mut regex = String::from("^");
    for c in pattern.chars() {
        match c {
            '%' => regex.push_str(".*"),
            '_' => regex.push('.'),
            c => regex.push_str(&regex::escape(c.encode_utf8(&mut [0; 4]))),
        }
    }
    regex.push('$');
    // Wildcards may match a newline within a file name.
    Ok(RegexBuilder::new(&regex)
        .case_insensitive(!case_sensitive)
        .dot_matches_new_line(true)
        .build()?)
}

/// Gets the value of a file attribute. Metadata the platform does not
/// provide is null. [`Predicate::compile`] has already rejected `*`.
#[expect(
    clippy::cast_precision_loss,
    reason = "sizes above 2^53 bytes (8 PiB) are not a practical concern"
)]
fn attribute_value(file: &FileResult, attribute: &FileAttribute) -> FileValue {
    match attribute {
        FileAttribute::Name => FileValue::String(file.name.clone()),
        FileAttribute::Path => FileValue::String(file.path.to_string_lossy().to_string()),
        FileAttribute::Size => FileValue::Number(file.size as f64),
        FileAttribute::Extension => FileValue::String(file.extension.clone().unwrap_or_default()),
        FileAttribute::Modified => FileValue::DateTime(file.modified),
        // Compare permission bits only; the raw mode also carries file-type bits.
        FileAttribute::Permissions => FileValue::Number(f64::from(file.permissions & 0o7777)),
        FileAttribute::IsDirectory => FileValue::Boolean(file.is_directory),
        FileAttribute::Owner => file
            .owner
            .clone()
            .map_or(FileValue::Null, FileValue::String),
        // Check if file has executable bit set for user
        FileAttribute::IsExecutable => FileValue::Boolean(file.permissions & 0o100 != 0),
        FileAttribute::Created => file.created.map_or(FileValue::Null, FileValue::DateTime),
        FileAttribute::Accessed => file.accessed.map_or(FileValue::Null, FileValue::DateTime),
        FileAttribute::IsSymlink => FileValue::Boolean(file.is_symlink),
        FileAttribute::Mode => FileValue::String(crate::output::ls_mode(file)),
        FileAttribute::All => unreachable!("compile rejects {attribute:?}"),
    }
}

/// Applies an operator to two values of the same ordered type. Sizes and
/// permission bits are whole numbers, so exact float equality is intended.
fn ordered<T: PartialOrd + ?Sized>(left: &T, operator: &ComparisonOperator, right: &T) -> bool {
    match operator {
        ComparisonOperator::Eq => left == right,
        ComparisonOperator::NotEq => left != right,
        ComparisonOperator::Lt => left < right,
        ComparisonOperator::LtEq => left <= right,
        ComparisonOperator::Gt => left > right,
        ComparisonOperator::GtEq => left >= right,
    }
}

/// Compares a file's value with a literal of the kind [`operand`] accepted.
/// A null on either side equals only another null.
fn compare_values(left: &FileValue, operator: &ComparisonOperator, right: &FileValue) -> bool {
    match (left, right) {
        (FileValue::String(l), FileValue::String(r)) => ordered(l, operator, r),
        (FileValue::Number(l), FileValue::Number(r)) => ordered(l, operator, r),
        (FileValue::DateTime(l), FileValue::DateTime(r)) => ordered(l, operator, r),
        (FileValue::Boolean(l), FileValue::Boolean(r)) => ordered(l, operator, r),
        (FileValue::Null, FileValue::Null) => *operator == ComparisonOperator::Eq,
        (FileValue::Null, _) | (_, FileValue::Null) => *operator == ComparisonOperator::NotEq,
        _ => unreachable!("compile checks operand types: {left:?} {operator} {right:?}"),
    }
}

/// Orders two values of one attribute, placing nulls after everything else.
fn order_values(left: &FileValue, right: &FileValue) -> Ordering {
    match (left, right) {
        (FileValue::String(l), FileValue::String(r)) => l.cmp(r),
        (FileValue::Number(l), FileValue::Number(r)) => l.total_cmp(r),
        (FileValue::DateTime(l), FileValue::DateTime(r)) => l.cmp(r),
        (FileValue::Boolean(l), FileValue::Boolean(r)) => l.cmp(r),
        (FileValue::Null, FileValue::Null) => Ordering::Equal,
        (FileValue::Null, _) => Ordering::Greater,
        (_, FileValue::Null) => Ordering::Less,
        _ => unreachable!("one attribute yields one kind: {left:?} {right:?}"),
    }
}

/// Attempts to parse a date string into a `DateTime<Utc>`.
///
/// Supports the formats `YYYY-MM-DD` and full RFC 3339 / ISO 8601 timestamps.
fn parse_date_string(s: &str) -> Result<DateTime<Utc>> {
    // Try full RFC 3339 first (e.g. "2025-01-01T00:00:00Z")
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return Ok(dt);
    }
    // Try bare date "YYYY-MM-DD" by appending midnight UTC
    if let Ok(dt) = format!("{s}T00:00:00Z").parse::<DateTime<Utc>>() {
        return Ok(dt);
    }
    Err(ExecutorError::TypeError(format!(
        "Cannot parse '{s}' as a date"
    )))
}

// Include the tests module
#[cfg(test)]
#[path = "executor_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "bug_regression_tests.rs"]
mod bug_regression_tests;
