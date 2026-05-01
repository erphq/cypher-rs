//! `cypher-rs` - openCypher front-end in Rust.
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
pub mod cost;
pub mod error;
pub mod optimize;
mod parser;
pub mod plan;
pub mod prune;
pub mod sema;

pub use ast::*;
pub use cost::{estimate, estimate_cost, CardinalityCostModel, CostModel, Estimate};
pub use error::ParseError;
pub use optimize::optimize;
pub use parser::parse;
pub use plan::{plan, Plan, PlanError, ProjectExpr, SortKey};
pub use prune::{output_columns, required_input_columns};
pub use sema::{
    analyze, analyze_with, AnalysisReport, PermissiveSchema, Schema, SemIssue, SemSeverity,
};
