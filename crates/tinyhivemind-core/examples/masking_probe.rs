//! Temporary probe: prints masking ranges for bodies read from stdin.
use std::io::Read;
use tinyhivemind_core::masking::{code_ranges, fenced_ranges};

fn decode(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn fmt(ranges: &[(usize, usize)]) -> String {
    ranges
        .iter()
        .map(|(a, b)| format!("{a},{b}"))
        .collect::<Vec<_>>()
        .join(";")
}

fn main() {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).unwrap();
    for line in input.lines() {
        if line.is_empty() {
            continue;
        }
        let body = decode(line);
        println!("{}|{}", fmt(&fenced_ranges(&body)), fmt(&code_ranges(&body)));
    }
}
