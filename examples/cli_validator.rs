//! CLI validator example.
//!
//! Reads a Nightjar rule from `argv[1]` and a JSON payload from stdin.
//! Prints `True` / `False` / the error and exits `0` for True, `1` for
//! False, `2` for Error.
//!
//! Run: `echo '{"revenue": 4200}' | cargo run --example cli_validator -- '(GE .revenue 0)'`

use nightjar_lang::{exec, ExecOptions, ExecResult};
use std::io::{self, Read};

fn main() {
    let mut args = std::env::args().skip(1);
    let rule = match args.next() {
        Some(r) => r,
        None => {
            eprintln!("usage: cli_validator '<rule>' < payload.json");
            std::process::exit(64); // EX_USAGE
        }
    };

    let mut raw = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut raw) {
        eprintln!("read stdin: {}", e);
        std::process::exit(74); // EX_IOERR
    }
    let data: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid JSON on stdin: {}", e);
            std::process::exit(65); // EX_DATAERR
        }
    };

    match exec(&rule, data, ExecOptions::default()) {
        ExecResult::True => {
            println!("True");
            std::process::exit(0);
        }
        ExecResult::False => {
            println!("False");
            std::process::exit(1);
        }
        ExecResult::Error(e) => {
            eprintln!("{:?} at {:?}: {}", e.code(), e.span(), e.message());
            std::process::exit(2);
        }
    }
}
