use std::env;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::process::ExitCode;

use csv_tsv_converter::{convert_csv_to_tsv, convert_tsv_to_csv};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_delimiter_accepts_tab_shorthand() {
        assert_eq!(parse_delimiter("\\t").unwrap(), b'\t');
    }

    #[test]
    fn parse_delimiter_rejects_multi_char() {
        assert!(parse_delimiter("ab").is_err());
    }
}
