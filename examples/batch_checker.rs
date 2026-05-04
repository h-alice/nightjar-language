//! Batch checker example.
//!
//! Streams a JSONL file, evaluates each line against a single rule, and
//! reports offending records with their error diagnostics.
//!
//! Run: `cargo run --example batch_checker -- payloads.jsonl`

use nightjar_lang::{exec, ExecOptions, ExecResult};
use std::fs::File;
use std::io::{BufRead, BufReader};

fn main() -> std::io::Result<()> {
    let rule = "(AND (GE .revenue 0) (LT .revenue 1000000))";
    let path = std::env::args()
        .nth(1)
        .expect("usage: batch_checker payloads.jsonl");
    let reader = BufReader::new(File::open(path)?);

    let mut failures = 0usize;
    let mut error_hit = 0usize;

    for (lineno, line) in reader.lines().enumerate() {
        let line = line?;
        let data: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("line {}: bad JSON: {}", lineno + 1, e);
                error_hit += 1;
                continue;
            }
        };
        match exec(rule, data, ExecOptions::default()) {
            ExecResult::True => {}
            ExecResult::False => {
                failures += 1;
                println!("line {}: False", lineno + 1);
            }
            ExecResult::Error(e) => {
                error_hit += 1;
                println!(
                    "line {}: Error {:?} at {:?}: {}",
                    lineno + 1,
                    e.code(),
                    e.span(),
                    e.message()
                );
            }
        }
    }

    eprintln!("done: {} failures, {} errors", failures, error_hit);
    Ok(())
}
