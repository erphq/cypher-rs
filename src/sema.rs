//! Semantic analysis for `cypher-rs` (v0.2 skeleton — see TODO.md).
//!
//! The semantic pass validates a parsed [`Query`] against a [`Schema`]:
//! checking that node labels, relationship types, and variable bindings
//! are consistent before query execution.

use crate::ast::Query;

/// A graph schema that the semantic analyser consults.
///
/// # TODO (v0.2)
/// - `fn node_labels(&self) -> &[&str]` — returns all known node labels.
/// - `fn rel_types(&self) -> &[&str]` — returns all known relationship types.
/// - `fn property_type(&self, label: &str, prop: &str) -> Option<PropType>`.
pub trait Schema {
    // TODO: add required methods
}

/// Errors produced by semantic analysis.
#[derive(Debug, thiserror::Error)]
pub enum SemaError {
    // TODO: add variants, e.g.:
    //   UndeclaredLabel(String),
    //   UnknownRelType(String),
    //   UnboundVariable(String),
    //   AmbiguousReturn(String),
    #[error("semantic analysis not yet implemented")]
    NotImplemented,
}

/// Entry point for semantic analysis.
///
/// Validates *query* against *schema*, returning `Ok(())` when the query
/// is semantically well-formed.
///
/// # TODO (v0.2)
/// - Walk the `MATCH` patterns; validate node labels and rel-types.
/// - Build a binding scope; check `WHERE` / `RETURN` variables are bound.
/// - Detect duplicate `RETURN` aliases.
pub fn analyze<S: Schema>(_query: &Query, _schema: &S) -> Result<(), SemaError> {
    // TODO: implement
    Err(SemaError::NotImplemented)
}
