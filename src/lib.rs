//! `cypher-rs` — openCypher front-end in Rust.
//!
//! Pre-v0. Parses a subset of openCypher: `MATCH`, `WHERE`, `RETURN`,
//! `LIMIT`, `SKIP` plus expressions (literals, variables, property
//! access, parameters, comparisons, boolean and arithmetic operators,
//! parenthesized sub-expressions).
//!
//! ```
//! use cypher_rs::parse;
//! let q = parse("MATCH (u:User) WHERE u.id = $uid RETURN u.name").unwrap();
//! assert_eq!(q.clauses.len(), 3);
//! ```
//!
//! Roadmap and scope: see the project README.

pub mod ast;
pub mod error;
mod parser;

pub use ast::*;
pub use error::ParseError;
pub use parser::parse;
