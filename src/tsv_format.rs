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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_field_is_unchanged() {
        assert_eq!(escape_field("hello"), "hello");
        assert_eq!(unescape_field("hello"), "hello");
    }

    #[test]
    fn escapes_tab_newline_cr_and_backslash() {
        assert_eq!(escape_field("a\tb\nc\rd\\e"), "a\\tb\\nc\\rd\\\\e");
    }

    #[test]
    fn unescapes_tab_newline_cr_and_backslash() {
        assert_eq!(unescape_field("a\\tb\\nc\\rd\\\\e"), "a\tb\nc\rd\\e");
    }

    #[test]
    fn unknown_escape_sequence_is_kept_literally() {
        // A backslash followed by something we don't recognize is not one
        // of our escapes, so both characters survive as written.
        assert_eq!(unescape_field("a\\xb"), "a\\xb");
    }

    #[test]
    fn trailing_backslash_with_nothing_after_it_survives() {
        assert_eq!(unescape_field("ab\\"), "ab\\");
    }

    #[test]
    fn round_trips_through_escape_and_unescape() {
        let original = "field\twith\ttabs\nand\nnewlines\rand\\backslashes";
        assert_eq!(unescape_field(&escape_field(original)), original);
    }

    #[test]
    fn write_record_joins_fields_with_tabs_and_escapes_them() {
        let fields = vec!["a\tb".to_string(), "plain".to_string(), "c\\d".to_string()];
        let mut out = Vec::new();
        write_record(&mut out, &fields).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "a\\tb\tplain\tc\\\\d\n");
    }
}
