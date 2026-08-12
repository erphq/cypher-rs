//! Tests for OPTIONAL MATCH: semantic analysis, plan display, and
//! projection pruning.
//!
//! Grammar-level parse tests live in tests/grammar_v0_2.rs.
//! Plan-lowering structure tests live in tests/plan.rs.
//! Cost-model tests live in tests/cost.rs.
//! Optimizer tests live in tests/optimize.rs.
//! This file fills the remaining gap: sema + prune + a few display
//! checks not covered elsewhere.

use std::collections::HashSet;

use cypher_rs::*;

// ---- semantic analysis ---------------------------------------------------

#[test]
fn optional_match_node_var_is_in_bindings() {
    let q = parse("MATCH (u:User) OPTIONAL MATCH (f:Friend) RETURN u, f").unwrap();
    let r = analyze(&q);
    assert!(
        !r.has_errors(),
        "expected no sema errors, got {:?}",
        r.issues
    );
    assert!(r.bindings.contains("u"), "expected 'u' in bindings");
    assert!(r.bindings.contains("f"), "expected 'f' in bindings");
}

#[test]
fn optional_match_rel_var_is_in_bindings() {
    let q =
        parse("MATCH (u:User) OPTIONAL MATCH (u)-[r:FOLLOWS]->(f:User) RETURN u, r, f").unwrap();
    let r = analyze(&q);
    assert!(
        !r.has_errors(),
        "expected no sema errors, got {:?}",
        r.issues
    );
    assert!(r.bindings.contains("r"), "expected rel var 'r' in bindings");
    assert!(r.bindings.contains("f"), "expected 'f' in bindings");
}

#[test]
fn unbound_variable_in_where_after_optional_match_is_flagged() {
    // 'x' is not introduced by any MATCH or OPTIONAL MATCH pattern.
    let q = parse("MATCH (u:User) OPTIONAL MATCH (f:Friend) WHERE x.id = 1 RETURN u, f").unwrap();
    let r = analyze(&q);
    assert!(r.has_errors(), "expected unbound-variable error for 'x'");
    let codes: Vec<_> = r.errors().map(|i| i.code).collect();
    assert!(
        codes.contains(&"unbound-variable"),
        "expected unbound-variable code, got {codes:?}"
    );
}

#[test]
fn return_referencing_optional_var_has_no_errors() {
    // 'f' comes only from the OPTIONAL branch; it must still be accepted
    // in the RETURN expression.
    let q = parse("MATCH (u:User) OPTIONAL MATCH (u)-[:FOLLOWS]->(f:User) RETURN f.name").unwrap();
    let r = analyze(&q);
    assert!(
        !r.has_errors(),
        "expected no errors when RETURN references optional var, got {:?}",
        r.issues
    );
}

#[test]
fn optional_match_with_schema_validates_rel_type() {
    struct OnlyFollows;
    impl Schema for OnlyFollows {
        fn has_rel_type(&self, ty: &str) -> bool {
            ty == "FOLLOWS"
        }
    }
    let bad = parse("MATCH (u:User) OPTIONAL MATCH (u)-[:INVENTED]->(f) RETURN u, f").unwrap();
    let r = analyze_with(&bad, &OnlyFollows);
    let codes: Vec<_> = r.errors().map(|i| i.code).collect();
    assert!(
        codes.contains(&"unknown-rel-type"),
        "expected unknown-rel-type for :INVENTED in OPTIONAL MATCH, got {codes:?}"
    );
}

// ---- plan display --------------------------------------------------------

#[test]
fn optional_match_plan_display_contains_optional() {
    let q = parse("MATCH (u:User) OPTIONAL MATCH (f:Friend) RETURN u, f").unwrap();
    let p = plan(&q).unwrap();
    let rendered = format!("{p}");
    assert!(
        rendered.contains("Optional"),
        "expected 'Optional' in plan display:\n{rendered}"
    );
}

#[test]
fn optional_match_after_cartesian_input_display_contains_both() {
    // Two regular MATCHes produce a Cartesian; the OPTIONAL MATCH wraps
    // that Cartesian in an Optional node.
    let q = parse("MATCH (a:A) MATCH (b:B) OPTIONAL MATCH (c:C) RETURN a, b, c").unwrap();
    let p = plan(&q).unwrap();
    let rendered = format!("{p}");
    assert!(
        rendered.contains("Optional"),
        "expected Optional in plan:\n{rendered}"
    );
    assert!(
        rendered.contains("Cartesian"),
        "expected Cartesian in plan:\n{rendered}"
    );
}

// ---- projection pruning --------------------------------------------------

#[test]
fn optional_match_output_columns_include_both_sides() {
    let q = parse("MATCH (u:User) OPTIONAL MATCH (f:Friend) RETURN u, f").unwrap();
    let p = plan(&q).unwrap();
    let cols = output_columns(&p);
    assert!(
        cols.contains("u"),
        "expected 'u' in output columns: {cols:?}"
    );
    assert!(
        cols.contains("f"),
        "expected 'f' in output columns: {cols:?}"
    );
}

#[test]
fn optional_match_output_columns_include_rel_var() {
    let q =
        parse("MATCH (u:User) OPTIONAL MATCH (u)-[r:FOLLOWS]->(f:User) RETURN u, r, f").unwrap();
    let p = plan(&q).unwrap();
    let cols = output_columns(&p);
    assert!(cols.contains("u"), "expected 'u': {cols:?}");
    assert!(cols.contains("r"), "expected 'r': {cols:?}");
    assert!(cols.contains("f"), "expected 'f': {cols:?}");
}

#[test]
fn optional_match_required_input_columns_passes_demand_through() {
    // required_input_columns on an Optional node returns outer_demand
    // unchanged (the Optional operator itself does not strip or filter
    // the demand; callers recurse into its sub-plans separately).
    let q = parse("MATCH (u:User) OPTIONAL MATCH (f:Friend) RETURN u, f").unwrap();
    let p = plan(&q).unwrap();
    let demand: HashSet<String> = ["u".to_string(), "f".to_string()].into();
    if let Plan::Project { input, .. } = &p {
        let req = required_input_columns(input, &demand);
        assert_eq!(
            req, demand,
            "Optional must pass demand through unchanged: got {req:?}"
        );
    } else {
        panic!("expected Project at root, got {p:?}");
    }
}
