use std::io::{self, BufRead, BufReader, Read, Write};

pub mod csv_format;
pub mod tsv_format;

/// Excel and a few other tools write a UTF-8 byte-order mark at the start
/// of a CSV export even though UTF-8 doesn't need one. Left alone it ends
/// up glued to the first field's contents (`\u{feff}name` instead of
/// `name`), so it's stripped before either format reader sees the stream.
/// A `fill_buf`/`consume` pair is enough to drop it without needing to
/// seek, which also means it works on stdin.
pub fn strip_bom<R: BufRead>(reader: &mut R) -> io::Result<()> {
    const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
    if reader.fill_buf()?.starts_with(&BOM) {
        reader.consume(BOM.len());
    }
    Ok(())
}

/// Streams the input one CSV record at a time; at no point is more than a
/// single record held in memory, regardless of how large the file is.
pub fn convert_csv_to_tsv<R: Read, W: Write>(
    mut reader: BufReader<R>,
    writer: &mut W,
    delimiter: u8,
    validate_header: bool,
) -> io::Result<()> {
    strip_bom(&mut reader)?;
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
pub fn convert_tsv_to_csv<R: Read, W: Write>(
    mut reader: BufReader<R>,
    writer: &mut W,
    delimiter: u8,
    validate_header: bool,
) -> io::Result<()> {
    strip_bom(&mut reader)?;
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
pub fn check_column_count(fields: &[String], expected_columns: &mut Option<usize>, row_number: u64) -> io::Result<()> {
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

    #[test]
    fn leading_bom_is_stripped_from_csv_input() {
        let input = "\u{feff}a,b,c\n1,2,3\n";
        assert_eq!(csv_to_tsv(input, b',', false).unwrap(), "a\tb\tc\n1\t2\t3\n");
    }

    #[test]
    fn leading_bom_is_stripped_from_tsv_input() {
        let input = "\u{feff}a\tb\tc\n1\t2\t3\n";
        assert_eq!(tsv_to_csv(input, b',', false).unwrap(), "a,b,c\n1,2,3\n");
    }

    #[test]
    fn bom_only_input_produces_no_records() {
        assert_eq!(csv_to_tsv("\u{feff}", b',', false).unwrap(), "");
        assert_eq!(tsv_to_csv("\u{feff}", b',', false).unwrap(), "");
    }
}
