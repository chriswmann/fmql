# 🔍 FMQL - File Management for People Who Talk to Databases

*Because talking to your files like they're a database is totally normal.*

[![Totally Real Build Status](https://img.shields.io/badge/build-passes%20when%20no%20one%20is%20looking-success)](https://github.com/chriswmann/fmql)
[![Sanity Status](https://img.shields.io/badge/sanity-questionable-yellow)](https://github.com/chriswmann/fmql)

## What is this madness?

FMQL (File Manager Query Language) lets you use SQL (sort of) to manage your files.

It puts the "Q" into "FML".

Yes it was mostly vibe-coded, I've been meaning to write this for months and this way I finally got round to it.

Edit: just discovered [fselect](https://fselect.rocks/) - if you want SQL file finding, use that!

⚠️ **WARNING**: This code is buggy as hell. Use at your own risk. Don't blame me when it deletes your homework or accidentally formats your hard drive. I'm still working on it, and by "working on it," I mean "not working on it very much at all because I don't have time right now".


```sql
SELECT * FROM ~/Documents WHERE size > 1000000 AND modified > '2025-01-01'
-- Translation: "Show me all those chonky files I created this year"
```

## ✨ Features That Will Make Your Files Feel Special

- **SQL-like Querying**: `SELECT * FROM ~/memes WHERE extension = 'jpg' AND name LIKE '%cat%'` because who needs GUI file search when you can type a novel?
- **Slightly Fast**: Traverses your filesystem faster than you can say "where did I put that file again?" ten times.
- **Detailed File Info**: Size, permissions, owner, modified/created/accessed times, symlinks. Like `ls -la` but dysfunctional and with a bad attitude.
- **Sorting, Limits and Columns**: `SELECT name, size FROM . ORDER BY size DESC LIMIT 10`, for when `du | sort | head` is too few characters.
- **Recursive Listing**: `WITH RECURSIVE SELECT ...`. Who needs `fd` when you can use fmql? (Everybody, actually.)
- **Output Formats**: `ls -l` style text, or JSON (`--format json`).
- **Safe-ish Updates**: Permission changes are validated before anything is touched and rolled back if one fails part way.

## 🔧 Installation (No Magic Required)

```bash
# The easy way
cargo install fmql

# The less way
git clone https://github.com/chriswmann/fmql.git
cd fmql
cargo build --release
```

## 📚 Usage: How to Pretend Your File System is a SQL Server DB from 1989?

```bash
# Find all text files in your Documents
fmql sql "SELECT * FROM ~/Documents WHERE extension = 'txt'"

# Find large images modified recently
fmql sql "SELECT * FROM ~/Pictures WHERE (extension = 'jpg' OR extension = 'png') AND size > 1000000 AND modified > '2023-06-01'"

# Find executable scripts (your personal army of automation)
fmql sql "SELECT * FROM ~/scripts WHERE is_executable = true"

# Update file permissions (because chmod is so 1970s)
fmql sql "UPDATE ~/scripts SET permissions = '755' WHERE extension = 'sh'"
```

### SQL Query Stuff

FMQL understands these SQL-ish commands:

- `SELECT * | attribute, ... FROM path [WHERE condition] [ORDER BY attribute [ASC|DESC], ...] [LIMIT n]`:
  Find matching files and directories. `SELECT *` prints `ls -l` style lines
  (or every field with `--format json`); naming attributes, as in
  `SELECT name, size FROM .`, prints just those columns, in that order. Without `ORDER BY`, each directory's
  entries are listed by name. `ORDER BY` accepts any filter attribute; nulls sort
  last in ascending order and first in descending order. `LIMIT` applies after
  sorting.
- `UPDATE path SET permissions = '755', owner = 'alice' [WHERE condition]`: Change
  permissions, ownership or both. Each attribute may be assigned once. The owner
  is a user name, looked up before any file changes; giving a file to another
  user normally needs root. Ownership changes first, because `chown` can clear
  setuid and setgid bits that the new permissions might set.
- `WHERE`: Comparisons (`=`, `!=`, `<>`, `<`, `<=`, `>`, `>=`), `AND`, `OR`,
  `NOT`, parentheses, `[NOT] BETWEEN`, `[NOT] LIKE`, `ILIKE`, and
  `REGEXP(attribute, 'pattern')` or `attribute [NOT] REGEXP 'pattern'`.
- `WITH RECURSIVE`: Prefix either statement to include descendants. Without it,
  both queries and updates visit only immediate children. The root directory is
  excluded. An update without `WHERE` changes every entry in that scope.

Keywords and attribute names are case-insensitive. `AND` binds more tightly than
`OR`; use parentheses to make grouping explicit. `LIKE` and `ILIKE` are both
case-insensitive, while string comparisons and regular expressions are
case-sensitive. LIKE uses `%` and `_` wildcards and has no `ESCAPE` support.

Supported filter attributes are `name`, `path`, `extension`, `owner` (strings),
`mode` (the `ls -l` string such as `-rwxr-x---`, so `mode LIKE '___x%'` finds
owner-executable entries), `size` (bytes), `permissions` (octal digits such as
`644`, with `=` or `!=` only),
`modified`, `created`, `accessed` (each a quoted `YYYY-MM-DD` or RFC 3339
timestamp), and `is_directory`, `is_executable`, `is_symlink` (booleans, with
equality or inequality, or on their own as in `WHERE NOT is_symlink`).
`is_executable` tests the owner's execute bit. `owner` is the user name for the
file's uid. Bare dates mean midnight UTC. Metadata the system cannot supply, such
as a creation time on a filesystem that does not record one or an owner missing
from the user database, is null: it fails every comparison except `!=`. Extension comparisons accept
either `'rs'` or `'.rs'`; a file without one has the extension `''`, which
both filters and `ORDER BY` treat as an ordinary (smallest) string. Permission filters and updates both read octal digits
from `0000` to `7777`, quoted or bare, as `chmod` does. A filter compares the
permission bits only, so `permissions = 644` matches a `rw-r--r--` file.

Paths may be absolute, relative, or start with `~` / `~/` for the home directory.
Double-quote paths with spaces or SQL punctuation, or a relative path that
starts with `WHERE` or `SET`:
`SELECT * FROM "/tmp/my files" WHERE name = 'it''s.txt'`. Double an embedded
double quote in a quoted path. String literals use single quotes and double an
embedded apostrophe. SQL comments and one optional trailing semicolon are allowed.

The parser uses `sqlparser` for tokens and expressions and requires the whole
query to be understood before execution. Unsupported queries return an error:
this includes expressions or aliases in the select list, grouping, `OFFSET`, joins,
subqueries, multiple statements and rename updates.
Malformed predicates, incompatible literal types and invalid regexes are also
rejected, including on empty directories. If changing one file's owner or
permissions fails, the files already changed get their previous owner and
permissions back and the error says so; if a restore also fails, the error
names the files left changed.

Symbolic links are listed but never followed into, and their attributes describe
the target (or the link itself when the target is missing). Updates skip
symbolic links, so `UPDATE` cannot change a file outside the directory it names.

## Examples

```bash
# Find all your unfinished projects
fmql sql "SELECT * FROM ~/projects WHERE modified < '2023-01-01' AND NOT name LIKE '%completed%'"

# List large memes
fmql sql "SELECT * FROM ~/Downloads WHERE name LIKE '%meme%' AND size > 1000000"

# Find screenshots in a time range
fmql sql "SELECT * FROM ~/Pictures WHERE name LIKE '%screenshot%' AND modified BETWEEN '2026-10-01T03:00:00Z' AND '2026-10-01T03:59:59Z'"

# Look for suspicious executables
fmql sql "SELECT * FROM ~/Downloads WHERE is_executable AND extension != 'sh'"

# The ten biggest files anywhere under your home directory
fmql sql "WITH RECURSIVE SELECT size, path FROM ~ WHERE NOT is_directory ORDER BY size DESC LIMIT 10"

# Files not owned by chris (unknown owners match too, since null != 'chris'), as JSON for jq
fmql sql "WITH RECURSIVE SELECT path, owner FROM . WHERE owner != 'chris'" --format json

# Symbolic links, oldest first
fmql sql "SELECT name, modified FROM /usr/local/bin WHERE is_symlink ORDER BY modified"
```

Text output for `SELECT *` looks like `ls -l`, one entry per line:

```text
-rwxr-xr-x  1832  2026-10-01 09:12  /home/you/scripts/backup.sh
drwxr-x---  4096  2026-09-14 18:40  /home/you/scripts/lib
```

Errors go to standard error with exit code 1, so standard output only ever
holds results.

## 🤝 Contributing

1. Question your life choices
2. Fork the repo
3. Create your feature branch (`git checkout -b feature/amazing-idea`)
4. Write code that doesn't make kittens cry
5. Commit your changes (`git commit -m 'Added that thing everyone wanted'`)
6. Push to the branch (`git push origin feature/amazing-idea`)
7. Open a Pull Request
8. Wait anxiously for feedback while refreshing GitHub every 30 seconds

## 📜 License

MIT License - Feel free to use this for whatever you like
