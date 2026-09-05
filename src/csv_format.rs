use std::io::{self, Read, Write};

/// Reads one CSV record at a time from an underlying byte stream.
///
/// Only the current record's bytes are ever held in memory, so this is
/// safe to point at an input of any size. It follows the common RFC 4180
/// conventions: fields are separated by a delimiter (comma by default,
/// but any single ASCII byte works), a field can be wrapped in double
/// quotes to contain the delimiter or a newline, and a doubled quote
/// inside a quoted field means a literal quote.
pub struct CsvReader<R: Read> {
    bytes: io::Bytes<R>,
    delimiter: u8,
    // Deciding whether a quote closes a field sometimes takes a
    // look at the following byte; if that byte turns out to start
    // the next token, it goes here instead of being read again.
    pending: Option<u8>,
}

impl<R: Read> CsvReader<R> {
    pub fn new(reader: R, delimiter: u8) -> Self {
        CsvReader { bytes: reader.bytes(), delimiter, pending: None }
    }

    fn next_byte(&mut self) -> io::Result<Option<u8>> {
        if let Some(b) = self.pending.take() {
            return Ok(Some(b));
        }
        match self.bytes.next() {
            None => Ok(None),
            Some(Ok(b)) => Ok(Some(b)),
            Some(Err(e)) => Err(e),
        }
    }

    /// Reads the next record into `fields`, clearing whatever was there
    /// before. Returns `Ok(false)` once the input is exhausted.
    pub fn read_record(&mut self, fields: &mut Vec<String>) -> io::Result<bool> {
        fields.clear();
        let mut field: Vec<u8> = Vec::new();
        let mut in_quotes = false;
        let mut field_was_quoted = false;
        let mut saw_any_byte = false;

        loop {
            let byte = match self.next_byte()? {
                None => {
                    if saw_any_byte {
                        fields.push(bytes_to_string(field));
                        return Ok(true);
                    }
                    return Ok(false);
                }
                Some(b) => b,
            };
            saw_any_byte = true;

            if in_quotes {
                if byte != b'"' {
                    field.push(byte);
                    continue;
                }
                // Either a closing quote or an escaped quote (""); the
                // only way to tell is to look at the byte right after it.
                match self.next_byte()? {
                    Some(b'"') => field.push(b'"'),
                    Some(next) if next == self.delimiter => {
                        in_quotes = false;
                        fields.push(bytes_to_string(std::mem::take(&mut field)));
                        field_was_quoted = false;
                    }
                    Some(b'\n') => {
                        fields.push(bytes_to_string(field));
                        return Ok(true);
                    }
                    Some(b'\r') => {
                        self.skip_lf_if_next()?;
                        fields.push(bytes_to_string(field));
                        return Ok(true);
                    }
                    Some(other) => {
                        in_quotes = false;
                        field.push(other);
                    }
                    None => {
                        fields.push(bytes_to_string(field));
                        return Ok(true);
                    }
                }
            } else {
                match byte {
                    b'"' if field.is_empty() && !field_was_quoted => {
                        in_quotes = true;
                        field_was_quoted = true;
                    }
                    other if other == self.delimiter => {
                        fields.push(bytes_to_string(std::mem::take(&mut field)));
                        field_was_quoted = false;
                    }
                    b'\n' => {
                        fields.push(bytes_to_string(field));
                        return Ok(true);
                    }
                    b'\r' => {
                        self.skip_lf_if_next()?;
                        fields.push(bytes_to_string(field));
                        return Ok(true);
                    }
                    other => field.push(other),
                }
            }
        }
    }

    /// After a bare \r, a following \n is part of the same line ending
    /// and gets dropped; anything else belongs to the next record and
    /// is pushed back so the next `read_record` call sees it first.
    fn skip_lf_if_next(&mut self) -> io::Result<()> {
        match self.next_byte()? {
            Some(b'\n') | None => Ok(()),
            Some(other) => {
                self.pending = Some(other);
                Ok(())
            }
        }
    }
}

fn bytes_to_string(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into_owned())
}

pub fn write_record<W: Write>(writer: &mut W, fields: &[String], delimiter: u8) -> io::Result<()> {
    for (i, field) in fields.iter().enumerate() {
        if i > 0 {
            writer.write_all(&[delimiter])?;
        }
        write_field(writer, field, delimiter)?;
    }
    writer.write_all(b"\n")
}

fn write_field<W: Write>(writer: &mut W, field: &str, delimiter: u8) -> io::Result<()> {
    // `delimiter` is validated at the CLI boundary to be a single ASCII
    // byte, so this cast back to `char` is exact.
    let delimiter = delimiter as char;
    let needs_quoting = field.contains(delimiter) || field.contains('"') || field.contains('\n') || field.contains('\r');
    if !needs_quoting {
        return writer.write_all(field.as_bytes());
    }
    writer.write_all(b"\"")?;
    let mut start = 0;
    for (idx, _) in field.match_indices('"') {
        writer.write_all(field[start..idx].as_bytes())?;
        writer.write_all(b"\"\"")?;
        start = idx + 1;
    }
    writer.write_all(field[start..].as_bytes())?;
    writer.write_all(b"\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_all(input: &str) -> Vec<Vec<String>> {
        read_all_with_delimiter(input, b',')
    }

    fn read_all_with_delimiter(input: &str, delimiter: u8) -> Vec<Vec<String>> {
        let mut reader = CsvReader::new(input.as_bytes(), delimiter);
        let mut records = Vec::new();
        let mut fields = Vec::new();
        while reader.read_record(&mut fields).unwrap() {
            records.push(fields.clone());
        }
        records
    }

    fn write_to_string(fields: &[&str]) -> String {
        write_to_string_with_delimiter(fields, b',')
    }

    fn write_to_string_with_delimiter(fields: &[&str], delimiter: u8) -> String {
        let owned: Vec<String> = fields.iter().map(|s| s.to_string()).collect();
        let mut out = Vec::new();
        write_record(&mut out, &owned, delimiter).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn empty_input_has_no_records() {
        assert_eq!(read_all(""), Vec::<Vec<String>>::new());
    }

    #[test]
    fn plain_fields_split_on_comma() {
        assert_eq!(read_all("a,b,c\n"), vec![vec!["a", "b", "c"]]);
    }

    #[test]
    fn trailing_record_without_final_newline() {
        assert_eq!(read_all("a,b,c"), vec![vec!["a", "b", "c"]]);
    }

    #[test]
    fn multiple_records() {
        assert_eq!(
            read_all("a,b\nc,d\n"),
            vec![vec!["a", "b"], vec!["c", "d"]]
        );
    }

    #[test]
    fn quoted_field_can_contain_comma() {
        assert_eq!(
            read_all("\"a,b\",c\n"),
            vec![vec!["a,b", "c"]]
        );
    }

    #[test]
    fn quoted_field_can_contain_newline() {
        assert_eq!(
            read_all("\"a\nb\",c\n"),
            vec![vec!["a\nb", "c"]]
        );
    }

    #[test]
    fn doubled_quote_in_quoted_field_is_literal_quote() {
        assert_eq!(
            read_all("\"say \"\"hi\"\"\"\n"),
            vec![vec!["say \"hi\""]]
        );
    }

    #[test]
    fn quote_mid_field_is_kept_literally() {
        // A '"' that doesn't start a field is not treated as CSV quoting.
        assert_eq!(read_all("ab\"cd,ef\n"), vec![vec!["ab\"cd", "ef"]]);
    }

    #[test]
    fn crlf_line_ending_is_stripped() {
        assert_eq!(read_all("a,b\r\nc,d\r\n"), vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn bare_cr_line_ending_is_stripped() {
        assert_eq!(read_all("a,b\rc,d\r"), vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn crlf_inside_quotes_is_preserved() {
        assert_eq!(read_all("\"a\r\nb\"\n"), vec![vec!["a\r\nb"]]);
    }

    #[test]
    fn empty_quoted_field() {
        assert_eq!(read_all("\"\",b\n"), vec![vec!["", "b"]]);
    }

    #[test]
    fn write_field_without_special_chars_is_unquoted() {
        assert_eq!(write_to_string(&["a", "b"]), "a,b\n");
    }

    #[test]
    fn write_field_with_comma_is_quoted() {
        assert_eq!(write_to_string(&["a,b", "c"]), "\"a,b\",c\n");
    }

    #[test]
    fn write_field_with_quote_doubles_it() {
        assert_eq!(write_to_string(&["say \"hi\""]), "\"say \"\"hi\"\"\"\n");
    }

    #[test]
    fn write_field_with_newline_is_quoted() {
        assert_eq!(write_to_string(&["a\nb"]), "\"a\nb\"\n");
    }

    #[test]
    fn round_trip_through_reader_and_writer() {
        let original = vec!["plain".to_string(), "has,comma".to_string(), "has\"quote".to_string()];
        let mut out = Vec::new();
        write_record(&mut out, &original, b',').unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(read_all(&text), vec![original]);
    }

    #[test]
    fn custom_delimiter_splits_fields() {
        assert_eq!(
            read_all_with_delimiter("a;b;c\n", b';'),
            vec![vec!["a", "b", "c"]]
        );
    }

    #[test]
    fn comma_is_not_special_with_a_custom_delimiter() {
        assert_eq!(
            read_all_with_delimiter("a,b;c\n", b';'),
            vec![vec!["a,b", "c"]]
        );
    }

    #[test]
    fn quoted_field_can_contain_a_custom_delimiter() {
        assert_eq!(
            read_all_with_delimiter("\"a;b\";c\n", b';'),
            vec![vec!["a;b", "c"]]
        );
    }

    #[test]
    fn write_field_quotes_on_custom_delimiter() {
        assert_eq!(
            write_to_string_with_delimiter(&["a;b", "c"], b';'),
            "\"a;b\";c\n"
        );
    }

    #[test]
    fn round_trip_with_custom_delimiter() {
        let original = vec!["plain".to_string(), "has;semicolon".to_string(), "has,comma".to_string()];
        let mut out = Vec::new();
        write_record(&mut out, &original, b';').unwrap();
        let text = String::from_utf8(out).unwrap();
        assert_eq!(read_all_with_delimiter(&text, b';'), vec![original]);
    }
}
