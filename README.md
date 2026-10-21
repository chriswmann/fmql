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
- **Detailed File Info**: Size, permissions, timestamps, and more. Like `ls -la` but dysfunctional and with a bad attitude.
- **Recursive Listing**: Who needs `fd` when you can use fmql? (Everybody, actually.)
- **Output Formats**: Text or JSON (`--format json`).

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

- `SELECT * FROM path [WHERE condition] [ORDER BY attribute [ASC|DESC], ...] [LIMIT n]`:
  Find matching files and directories. Without `ORDER BY`, each directory's
  entries are listed by name. `ORDER BY` accepts any filter attribute; nulls sort
  last in ascending order and first in descending order. `LIMIT` applies after
  sorting.
- `UPDATE path SET permissions = '755' [WHERE condition]`: Change permissions.
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
`size` (bytes), `permissions` (octal digits such as `644`, with `=` or `!=` only),
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
this includes projections other than `*`, grouping, `OFFSET`, joins,
subqueries, multiple statements and ownership/rename updates.
Malformed predicates, incompatible literal types and invalid regexes are also
rejected, including on empty directories. Filesystem failures during an accepted
update can still leave earlier files changed; updates are not transactional.

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
fmql sql "SELECT * FROM ~/Downloads WHERE is_executable = true AND extension != 'sh'"
```

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
