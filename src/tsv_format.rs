use std::io::{self, Write};

/// This tool's TSV dialect has no quoting at all - a tab always ends a
/// field and a newline always ends a record. Anything that would collide
/// with that (a literal tab, newline, carriage return, or backslash) gets
/// backslash-escaped instead, the same convention used by `mysqldump` and
/// Postgres's `COPY ... TO`. That keeps a record on exactly one line, so
/// the reader side can just read one line at a time.
pub fn write_record<W: Write>(writer: &mut W, fields: &[String]) -> io::Result<()> {
    for (i, field) in fields.iter().enumerate() {
        if i > 0 {
            writer.write_all(b"\t")?;
        }
        writer.write_all(escape_field(field).as_bytes())?;
    }
    writer.write_all(b"\n")
}

pub fn escape_field(field: &str) -> String {
    if !field.contains(['\\', '\t', '\n', '\r']) {
        return field.to_string();
    }
    let mut out = String::with_capacity(field.len());
    for ch in field.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

pub fn unescape_field(field: &str) -> String {
    if !field.contains('\\') {
        return field.to_string();
    }
    let mut out = String::with_capacity(field.len());
    let mut chars = field.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => {
                // not one of our escapes - keep it literally rather than
                // silently eating a backslash that wasn't ours
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}
