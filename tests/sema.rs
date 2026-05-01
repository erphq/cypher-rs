//! TODO integration tests for the semantic analyser (v0.2 milestone).
//!
//! See TODO.md for the full checklist. All tests are `#[ignore]` until
//! `analyze()` is implemented.

use cypher_rs::parse;
use cypher_rs::sema::{analyze, Schema, SemaError};

struct EmptySchema;
impl Schema for EmptySchema {}

#[test]
#[ignore = "TODO: implement analyze()"]
fn analyze_valid_query_returns_ok() {
    let q = parse("MATCH (n:User) RETURN n.name").unwrap();
    assert!(analyze(&q, &EmptySchema).is_ok());
}

#[test]
#[ignore = "TODO: implement analyze() + a schema that rejects unknown labels"]
fn analyze_unknown_label_returns_error() {
    // TODO: define a RealSchema that only accepts known labels,
    //   then verify analyze returns Err(SemaError::UndeclaredLabel(_)).
    todo!()
}

#[test]
#[ignore = "TODO: implement variable-scope analysis"]
fn analyze_unbound_variable_returns_error() {
    // MATCH (n) RETURN m  — `m` is never bound
    let q = parse("MATCH (n) RETURN m").unwrap();
    let result = analyze(&q, &EmptySchema);
    // Placeholder assertion — replace with the real error variant.
    assert!(matches!(result, Err(SemaError::NotImplemented)));
}

#[test]
#[ignore = "TODO: implement duplicate-alias detection"]
fn analyze_duplicate_return_alias_returns_error() {
    // TODO: construct a query with a duplicated AS alias and verify the error.
    todo!()
}
