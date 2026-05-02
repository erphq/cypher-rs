//! `cypher-mcp` - stdio MCP server for the `cypher-rs` front-end.
//!
//! Built with `cargo build --release --features mcp --bin cypher-mcp`.
//! See `src/mcp.rs` for the protocol layer and the tool surface.

use cypher_rs::mcp;
use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(response) = mcp::handle_request(line) {
            writeln!(stdout, "{}", response).ok();
            stdout.flush().ok();
        }
    }
}
