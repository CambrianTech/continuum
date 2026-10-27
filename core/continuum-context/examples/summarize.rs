//! Print an item's summary level the way the view would get it, and what it costs: the receipt
//! for any shape on real content (a captured read, a build log).
//!
//! `cargo run -p continuum-context --example summarize -- <code|output|list> <file>`
//!
//! `code` parses the hands gutter (`  N | code`), the format `code/read` renders.

use continuum_context::levels::{summarize, Shape, LEVEL_RATIO};

fn gutter(line: &str) -> Option<(u32, &str)> {
    let (number, code) = line.split_once(" | ")?;
    Some((number.trim().parse().ok()?, code))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let shape = args.next().ok_or("usage: summarize <code|output|list> <file>")?;
    let path = args.next().ok_or("usage: summarize <code|output|list> <file>")?;
    let full = std::fs::read_to_string(path)?;
    let shape = match shape.as_str() {
        "code" => Shape::Code(&gutter),
        "output" => Shape::Output,
        "list" => Shape::List,
        other => return Err(format!("unknown shape {other}").into()),
    };
    let chars = full.chars().count();
    match summarize(&full, shape) {
        Some(summary) => {
            let level = summary.chars().count();
            println!("full {chars} chars, summary {level} chars ({:.1}% of full; bound {:.0}%)", 100.0 * level as f64 / chars as f64, 100.0 / LEVEL_RATIO as f64);
            println!("{}", summary.lines().take(12).collect::<Vec<_>>().join("\n"));
        }
        None => println!("full {chars} chars: no summary within 1/{LEVEL_RATIO}; the view chooses whole or pointer"),
    }
    Ok(())
}
