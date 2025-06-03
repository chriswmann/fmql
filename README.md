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
- **Sophisticated Sorting**: By name, size, modified date, or type.
- **Grouping Options**: Group by extension, permissions, or name patterns.
- **Recursive Listing**: Who needs `fd` when you can use fmql? (Everybody, actually.)
- **Multiple Output Formats**: Text, table, or JSON. Text and table are basically the same so "two output formats" is more accurate.

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
fmql sql "SELECT * FROM ~/scripts WHERE permissions LIKE '%x%'"

# Update file permissions (because chmod is so 1970s)
fmql sql "UPDATE ~/scripts SET permissions = '755' WHERE extension = 'sh'"
```

### SQL Query Stuff

FMQL understands these SQL-ish commands:

- `SELECT`: Find files matching specific conditions
- `UPDATE`: Modify file attributes (permissions, etc.)
- `WHERE`: Filter with conditions (`=`, `>`, `<`, `LIKE`, `REGEXP`)
- `WITH RECURSIVE`: Recursively search directories (prepare for deep dives)

## Examples

```bash
# Find all your unfinished projects
fmql sql "SELECT * FROM ~/projects WHERE modified < '2023-01-01' AND NOT name LIKE '%completed%'"

# List all memes sorted by size
fmql sql "SELECT * FROM ~/Downloads WHERE name LIKE '%meme%' ORDER BY size DESC"

# Find screenshots from 3am
fmql sql "SELECT * FROM ~/Pictures WHERE name LIKE '%screenshot%' AND modified LIKE '%03:%'"

# Look for suspicious executables
fmql sql "SELECT * FROM ~/Downloads WHERE is_executable = true AND NOT permission = '755'"
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
