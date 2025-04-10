# fmql

A fast and feature-rich file manager written in Rust.

## Features

- Fast file system traversal
- Detailed file information
- Multiple sorting options
- File grouping by various criteria
- Recursive directory listing
- Hidden file filtering
- Multiple output formats
- Total size calculation

## Installation

```bash
cargo install fmql
```

## Usage

```bash
fmql [OPTIONS] [PATH]
```

### Options

- `-a, --all`: Show hidden files (files starting with '.')
- `-l, --long`: Use detailed view (shows more information)
- `-s, --sort <SORT_OPTION>`: Sort by: name, size, modified, type (default: name)
- `-r, --recursive`: Recursively list directories
- `-t, --total`: Show total size of all files
- `-g, --group-by <GROUP_OPTION>`: Group totals by: none, folder, all-folders, extension, permissions, executable, name-starts-with, name-contains, name-ends-with (default: none)
- `--pattern <PATTERN>`: Pattern for name-based grouping (required for name-starts-with, name-contains, name-ends-with)
- `-f, --format <FORMAT>`: Output format: text, table (default: text)

### Examples

```bash
# List files in current directory
fmql

# Show hidden files
fmql -a

# Sort by size
fmql -s size

# Group by extension
fmql -g extension

# Recursive listing
fmql -r

# Custom path
fmql /path/to/directory
```

## Grouping Options

The `--group-by` option supports the following values:

- `none`: No grouping (default)
- `folder`: Group by parent folder
- `all-folders`: Group by all parent folders
- `extension`: Group by file extension
- `permissions`: Group by file permissions
- `executable`: Group by executable status
- `name-starts-with`: Group by name prefix (requires --pattern)
- `name-contains`: Group by name substring (requires --pattern)
- `name-ends-with`: Group by name suffix (requires --pattern)

## Output Formats

### Text Format
```
drwxr-xr-x  user 4096 2024-03-20 10:00:00 directory/
-rw-r--r--  user 1234 2024-03-20 10:00:00 file.txt
```

### Table Format
```
Permissions  Owner  Size  Modified            Name
drwxr-xr-x  user   4096  2024-03-20 10:00:00 directory/
-rw-r--r--  user   1234  2024-03-20 10:00:00 file.txt
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details. 