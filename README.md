# csv-tsv-converter

A small command-line tool that converts between CSV and TSV, streaming
through the input a record at a time instead of loading the whole file
into memory. I wanted something I could point at a multi-gigabyte export
without watching it eat all the RAM on the box.

No dependencies - just the Rust standard library.

## Build

```
cargo build --release
```

## Usage

```
csv-tsv-converter [-d|--delimiter <char>] [--header] <csv2tsv|tsv2csv> <input> [output]
```

Use `-` for input or output to mean stdin/stdout. If output is omitted it
defaults to stdout.

Pass `--header` to treat the first row as a header and fail the conversion
as soon as a later row has a different number of columns. Without it,
ragged rows are passed through as-is - the same as most CSV tools default
to.

By default the CSV side is comma-delimited, as usual. Pass `-d` or
`--delimiter` with a single character to use something else - semicolon
and pipe-delimited exports are common enough to run into. `\t` is accepted
as shorthand for a literal tab, since typing one on a command line is
awkward:

```
$ csv-tsv-converter -d ';' csv2tsv european-export.csv out.tsv
```

The delimiter only affects the CSV side; the TSV dialect always uses tabs
(see below).

```
$ cat people.csv
name,role,notes
"Ada Lovelace",engineer,"wrote the first algorithm, comma included"
Grace Hopper,engineer,"debugged literal bugs"

$ csv-tsv-converter csv2tsv people.csv people.tsv
$ cat people.tsv
name	role	notes
Ada Lovelace	engineer	wrote the first algorithm, comma included
Grace Hopper	engineer	debugged literal bugs

$ csv-tsv-converter tsv2csv people.tsv -
name,role,notes
Ada Lovelace,engineer,"wrote the first algorithm, comma included"
Grace Hopper,engineer,debugged literal bugs
```

It also works as a pipe:

```
curl -s https://example.com/export.csv | csv-tsv-converter csv2tsv - out.tsv
```

## Format notes

CSV parsing follows the usual RFC 4180 conventions: fields are separated
by a delimiter (comma by default, configurable with `--delimiter`), a
field can be wrapped in double quotes to hold the delimiter or a newline,
and a doubled quote (`""`) inside a quoted field means a literal quote
character. Both `\n` and `\r\n` line endings are accepted.

The TSV side has no quoting mechanism at all - a tab always ends a field
and a newline always ends a record, so a record is always exactly one
line. Any tab, newline, carriage return, or backslash that shows up
inside a field is backslash-escaped (`\t`, `\n`, `\r`, `\\`) on the way
out and unescaped on the way back in. This is the same convention
`mysqldump` and PostgreSQL's `COPY ... TO` use for tab-delimited output,
so files from either of those should round-trip cleanly.

## How the streaming works

`csv2tsv` reads the input byte by byte through a small state machine that
knows when it's inside a quoted field, and hands back one completed
record (a `Vec<String>`) at a time. `tsv2csv` reads one line at a time,
since a TSV record here is always a single line by construction. Either
way, memory use stays proportional to the size of the widest single
record, not the size of the file.

## License

MIT, see LICENSE.
