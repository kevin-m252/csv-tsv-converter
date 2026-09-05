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

    match run(mode, input_path, output_path, delimiter) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn print_usage(program: &str) {
    eprintln!("usage: {} [-d|--delimiter <char>] <csv2tsv|tsv2csv> <input> [output]", program);
    eprintln!("       '-' for input or output means stdin/stdout");
    eprintln!("       --delimiter sets the CSV-side field separator (default ',')");
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

fn run(mode: &str, input_path: &str, output_path: &str, delimiter: u8) -> io::Result<()> {
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
        "csv2tsv" => convert_csv_to_tsv(reader, &mut writer, delimiter)?,
        "tsv2csv" => convert_tsv_to_csv(reader, &mut writer, delimiter)?,
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
fn convert_csv_to_tsv<R: Read, W: Write>(reader: BufReader<R>, writer: &mut W, delimiter: u8) -> io::Result<()> {
    let mut csv_reader = csv_format::CsvReader::new(reader, delimiter);
    let mut fields: Vec<String> = Vec::new();
    while csv_reader.read_record(&mut fields)? {
        tsv_format::write_record(writer, &fields)?;
    }
    Ok(())
}

/// Streams the input one line at a time; this TSV dialect never puts a
/// raw newline inside a field, so a line is always exactly one record.
fn convert_tsv_to_csv<R: Read, W: Write>(mut reader: BufReader<R>, writer: &mut W, delimiter: u8) -> io::Result<()> {
    let mut line = String::new();
    let mut fields: Vec<String> = Vec::new();
    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            break;
        }

        let trimmed = line.strip_suffix('\n').unwrap_or(&line);
        let trimmed = trimmed.strip_suffix('\r').unwrap_or(trimmed);

        fields.clear();
        for raw_field in trimmed.split('\t') {
            fields.push(tsv_format::unescape_field(raw_field));
        }
        csv_format::write_record(writer, &fields, delimiter)?;
    }
    Ok(())
}
