use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::process::ExitCode;

mod csv_format;
mod tsv_format;

fn main() -> ExitCode {
    let raw_args: Vec<String> = env::args().collect();
    let program = raw_args.first().cloned().unwrap_or_else(|| "csv-tsv-converter".to_string());

    let mut delimiter: u8 = b',';
    let mut validate_header: bool = false;
    let mut positional: Vec<String> = Vec::new();
    let mut args = raw_args.into_iter().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-d" | "--delimiter" => {
                let value = match args.next() {
                    Some(v) => v,
                    None => {
                        eprintln!("error: {} requires a value", arg);
                        print_usage(&program);
                        return ExitCode::FAILURE;
                    }
                };
                match parse_delimiter(&value) {
                    Ok(b) => delimiter = b,
                    Err(e) => {
                        eprintln!("error: {}", e);
                        return ExitCode::FAILURE;
                    }
                }
            }
            "--header" => validate_header = true,
            _ => positional.push(arg),
        }
    }

    if positional.len() < 2 || positional.len() > 3 {
        print_usage(&program);
        return ExitCode::FAILURE;
    }

    let mode = positional[0].as_str();
    let input_path = positional[1].as_str();
    let output_path = positional.get(2).map(String::as_str).unwrap_or("-");

    match run(mode, input_path, output_path, delimiter, validate_header) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn print_usage(program: &str) {
    eprintln!("usage: {} [-d|--delimiter <char>] [--header] <csv2tsv|tsv2csv> <input> [output]", program);
    eprintln!("       '-' for input or output means stdin/stdout");
    eprintln!("       --delimiter sets the CSV-side field separator (default ',')");
    eprintln!("       --header fails the conversion if any row's column count");
    eprintln!("                differs from the first row's");
}

/// The delimiter has to be exactly one byte because the CSV reader compares
/// it against bytes read one at a time; `\t` is accepted as shorthand since
/// typing a literal tab on a command line is awkward.
fn parse_delimiter(value: &str) -> Result<u8, String> {
    if value == "\\t" {
        return Ok(b'\t');
    }
    let bytes = value.as_bytes();
    if bytes.len() != 1 {
        return Err(format!("delimiter must be a single ASCII character, got '{}'", value));
    }
    Ok(bytes[0])
}

fn run(mode: &str, input_path: &str, output_path: &str, delimiter: u8, validate_header: bool) -> io::Result<()> {
    let input: Box<dyn Read> = if input_path == "-" {
        Box::new(io::stdin())
    } else {
        Box::new(File::open(input_path)?)
    };
    let output: Box<dyn Write> = if output_path == "-" {
        Box::new(io::stdout())
    } else {
        Box::new(File::create(output_path)?)
    };

    let reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);

    match mode {
        "csv2tsv" => convert_csv_to_tsv(reader, &mut writer, delimiter, validate_header)?,
        "tsv2csv" => convert_tsv_to_csv(reader, &mut writer, delimiter, validate_header)?,
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown mode '{}', expected csv2tsv or tsv2csv", other),
            ))
        }
    }

    writer.flush()
}

/// Streams the input one CSV record at a time; at no point is more than a
/// single record held in memory, regardless of how large the file is.
fn convert_csv_to_tsv<R: Read, W: Write>(
    reader: BufReader<R>,
    writer: &mut W,
    delimiter: u8,
    validate_header: bool,
) -> io::Result<()> {
    let mut csv_reader = csv_format::CsvReader::new(reader, delimiter);
    let mut fields: Vec<String> = Vec::new();
    let mut expected_columns: Option<usize> = None;
    let mut row_number: u64 = 0;
    while csv_reader.read_record(&mut fields)? {
        row_number += 1;
        if validate_header {
            check_column_count(&fields, &mut expected_columns, row_number)?;
        }
        tsv_format::write_record(writer, &fields)?;
    }
    Ok(())
}

/// Streams the input one line at a time; this TSV dialect never puts a
/// raw newline inside a field, so a line is always exactly one record.
fn convert_tsv_to_csv<R: Read, W: Write>(
    mut reader: BufReader<R>,
    writer: &mut W,
    delimiter: u8,
    validate_header: bool,
) -> io::Result<()> {
    let mut line = String::new();
    let mut fields: Vec<String> = Vec::new();
    let mut expected_columns: Option<usize> = None;
    let mut row_number: u64 = 0;
    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            break;
        }
        row_number += 1;

        let trimmed = line.strip_suffix('\n').unwrap_or(&line);
        let trimmed = trimmed.strip_suffix('\r').unwrap_or(trimmed);

        fields.clear();
        for raw_field in trimmed.split('\t') {
            fields.push(tsv_format::unescape_field(raw_field));
        }
        if validate_header {
            check_column_count(&fields, &mut expected_columns, row_number)?;
        }
        csv_format::write_record(writer, &fields, delimiter)?;
    }
    Ok(())
}

/// With `--header`, the first row seen sets the expected column count and
/// every later row must match it exactly; this is the only sanity check
/// that's meaningful without knowing anything about the schema.
fn check_column_count(fields: &[String], expected_columns: &mut Option<usize>, row_number: u64) -> io::Result<()> {
    match *expected_columns {
        None => *expected_columns = Some(fields.len()),
        Some(expected) if expected != fields.len() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "row {} has {} column(s), expected {} (from header row)",
                    row_number,
                    fields.len(),
                    expected
                ),
            ));
        }
        Some(_) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csv_to_tsv(input: &str, delimiter: u8, validate_header: bool) -> io::Result<String> {
        let reader = BufReader::new(input.as_bytes());
        let mut out = Vec::new();
        convert_csv_to_tsv(reader, &mut out, delimiter, validate_header)?;
        Ok(String::from_utf8(out).unwrap())
    }

    fn tsv_to_csv(input: &str, delimiter: u8, validate_header: bool) -> io::Result<String> {
        let reader = BufReader::new(input.as_bytes());
        let mut out = Vec::new();
        convert_tsv_to_csv(reader, &mut out, delimiter, validate_header)?;
        Ok(String::from_utf8(out).unwrap())
    }

    #[test]
    fn parse_delimiter_accepts_tab_shorthand() {
        assert_eq!(parse_delimiter("\\t").unwrap(), b'\t');
    }

    #[test]
    fn parse_delimiter_rejects_multi_char() {
        assert!(parse_delimiter("ab").is_err());
    }

    #[test]
    fn header_check_passes_when_uniform() {
        let result = csv_to_tsv("a,b,c\n1,2,3\n4,5,6\n", b',', true);
        assert_eq!(result.unwrap(), "a\tb\tc\n1\t2\t3\n4\t5\t6\n");
    }

    #[test]
    fn header_check_fails_on_short_row() {
        let err = csv_to_tsv("a,b,c\n1,2\n", b',', true).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("row 2"));
    }

    #[test]
    fn header_check_fails_on_long_row() {
        let err = csv_to_tsv("a,b,c\n1,2,3,4\n", b',', true).unwrap_err();
        assert!(err.to_string().contains("row 2"));
    }

    #[test]
    fn header_check_off_by_default_allows_ragged_rows() {
        let result = csv_to_tsv("a,b,c\n1,2\n", b',', false);
        assert_eq!(result.unwrap(), "a\tb\tc\n1\t2\n");
    }

    #[test]
    fn header_check_applies_to_tsv_to_csv_too() {
        let err = tsv_to_csv("a\tb\tc\n1\t2\n", b',', true).unwrap_err();
        assert!(err.to_string().contains("row 2"));
    }
}
