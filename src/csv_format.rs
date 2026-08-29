use std::io::{self, Read, Write};

/// Reads one CSV record at a time from an underlying byte stream.
///
/// Only the current record's bytes are ever held in memory, so this is
/// safe to point at an input of any size. It follows the common RFC 4180
/// conventions: fields are separated by commas, a field can be wrapped in
/// double quotes to contain commas or newlines, and a doubled quote inside
/// a quoted field means a literal quote.
pub struct CsvReader<R: Read> {
    bytes: io::Bytes<R>,
    // Deciding whether a quote closes a field sometimes takes a
    // look at the following byte; if that byte turns out to start
    // the next token, it goes here instead of being read again.
    pending: Option<u8>,
}

impl<R: Read> CsvReader<R> {
    pub fn new(reader: R) -> Self {
        CsvReader { bytes: reader.bytes(), pending: None }
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
                    Some(b',') => {
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
                    b',' => {
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

pub fn write_record<W: Write>(writer: &mut W, fields: &[String]) -> io::Result<()> {
    for (i, field) in fields.iter().enumerate() {
        if i > 0 {
            writer.write_all(b",")?;
        }
        write_field(writer, field)?;
    }
    writer.write_all(b"\n")
}

fn write_field<W: Write>(writer: &mut W, field: &str) -> io::Result<()> {
    let needs_quoting = field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r');
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
