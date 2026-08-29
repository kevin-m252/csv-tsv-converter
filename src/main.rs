use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::process::ExitCode;

mod csv_format;
mod tsv_format;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 || args.len() > 4 {
        print_usage(&args);
        return ExitCode::FAILURE;
    }

    let mode = args[1].as_str();
    let input_path = args[2].as_str();
    let output_path = args.get(3).map(String::as_str).unwrap_or("-");

    match run(mode, input_path, output_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn print_usage(args: &[String]) {
    let program = args.first().map(String::as_str).unwrap_or("csv-tsv-converter");
    eprintln!("usage: {} <csv2tsv|tsv2csv> <input> [output]", program);
    eprintln!("       '-' for input or output means stdin/stdout");
}

fn run(mode: &str, input_path: &str, output_path: &str) -> io::Result<()> {
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
        "csv2tsv" => convert_csv_to_tsv(reader, &mut writer)?,
        "tsv2csv" => convert_tsv_to_csv(reader, &mut writer)?,
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
fn convert_csv_to_tsv<R: Read, W: Write>(reader: BufReader<R>, writer: &mut W) -> io::Result<()> {
    let mut csv_reader = csv_format::CsvReader::new(reader);
    let mut fields: Vec<String> = Vec::new();
    while csv_reader.read_record(&mut fields)? {
        tsv_format::write_record(writer, &fields)?;
    }
    Ok(())
}

/// Streams the input one line at a time; this TSV dialect never puts a
/// raw newline inside a field, so a line is always exactly one record.
fn convert_tsv_to_csv<R: Read, W: Write>(mut reader: BufReader<R>, writer: &mut W) -> io::Result<()> {
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
        csv_format::write_record(writer, &fields)?;
    }
    Ok(())
}
