//! Generates a synthetic CSV file on disk and times both conversion
//! directions against it through the same file-based path the CLI uses,
//! so the numbers reflect real I/O rather than an in-memory shortcut.
//!
//! Run with `cargo run --release --example bench [row-count]`. Row count
//! defaults to one million if not given.

use std::env;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Write};
use std::time::{Duration, Instant};

use csv_tsv_converter::{convert_csv_to_tsv, convert_tsv_to_csv, csv_format};

const CITIES: [&str; 5] = ["Amsterdam", "Nairobi", "Osaka", "Toronto", "Lima"];

fn main() -> io::Result<()> {
    let rows: u64 = env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1_000_000);

    let dir = env::temp_dir();
    let csv_path = dir.join("csv-tsv-converter-bench.csv");
    let tsv_path = dir.join("csv-tsv-converter-bench.tsv");
    let roundtrip_path = dir.join("csv-tsv-converter-bench-roundtrip.csv");

    println!("generating {} rows of synthetic csv at {}", rows, csv_path.display());
    write_bench_csv(&csv_path, rows)?;
    let csv_bytes = fs::metadata(&csv_path)?.len();

    let csv_to_tsv_elapsed = time_it(|| {
        let reader = BufReader::new(File::open(&csv_path)?);
        let mut writer = BufWriter::new(File::create(&tsv_path)?);
        convert_csv_to_tsv(reader, &mut writer, b',', false)?;
        writer.flush()
    })?;
    let tsv_bytes = fs::metadata(&tsv_path)?.len();

    let tsv_to_csv_elapsed = time_it(|| {
        let reader = BufReader::new(File::open(&tsv_path)?);
        let mut writer = BufWriter::new(File::create(&roundtrip_path)?);
        convert_tsv_to_csv(reader, &mut writer, b',', false)?;
        writer.flush()
    })?;

    println!();
    report("csv2tsv", rows, csv_bytes, csv_to_tsv_elapsed);
    report("tsv2csv", rows, tsv_bytes, tsv_to_csv_elapsed);

    for path in [&csv_path, &tsv_path, &roundtrip_path] {
        fs::remove_file(path).ok();
    }

    Ok(())
}

/// A mix of plain fields and fields that force the quoting path (an
/// embedded comma, an embedded quote, an embedded newline), so the
/// benchmark exercises more than just the fast unquoted case.
fn write_bench_csv(path: &std::path::Path, rows: u64) -> io::Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);

    let header: Vec<String> = ["id", "name", "city", "notes", "score"].iter().map(|s| s.to_string()).collect();
    csv_format::write_record(&mut writer, &header, b',')?;

    for i in 0..rows {
        let notes = if i % 11 == 0 {
            format!("multi\nline note #{}", i)
        } else if i % 5 == 0 {
            format!("note with, a comma and \"quotes\" #{}", i)
        } else {
            format!("plain note {}", i)
        };
        let fields = vec![
            i.to_string(),
            format!("person-{}", i),
            CITIES[(i as usize) % CITIES.len()].to_string(),
            notes,
            format!("{:.2}", (i % 1000) as f64 / 7.0),
        ];
        csv_format::write_record(&mut writer, &fields, b',')?;
    }

    writer.flush()
}

fn time_it<F: FnOnce() -> io::Result<()>>(f: F) -> io::Result<Duration> {
    let start = Instant::now();
    f()?;
    Ok(start.elapsed())
}

fn report(direction: &str, rows: u64, bytes: u64, elapsed: Duration) {
    let secs = elapsed.as_secs_f64();
    let mb = bytes as f64 / (1024.0 * 1024.0);
    println!(
        "{:<8} {:>10} rows  {:>9.1} MB in  {:>7.3}s  ({:>10.0} rows/s, {:>7.1} MB/s)",
        direction,
        rows,
        mb,
        secs,
        rows as f64 / secs,
        mb / secs,
    );
}
