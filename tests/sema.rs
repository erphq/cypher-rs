//! v0.3 semantic analyzer tests.

use std::collections::HashSet;

use cypher_rs::*;

#[test]
fn collects_bindings_from_node_patterns() {
    let q = parse("MATCH (u:User) RETURN u").unwrap();
    let r = analyze(&q);
    assert_eq!(
        r.bindings,
        HashSet::from(["u".to_string()]),
        "expected only `u`, got {:?}",
        r.bindings
    );
    assert!(!r.has_errors());
}

#[test]
fn collects_bindings_from_relationship_patterns() {
    let q = parse("MATCH (u)-[r:KNOWS]->(v) RETURN u, v").unwrap();
    let r = analyze(&q);
    assert_eq!(
        r.bindings,
        HashSet::from(["u".to_string(), "r".to_string(), "v".to_string()])
    );
}

#[test]
fn flags_unbound_variable_in_where() {
    let q = parse("MATCH (u) WHERE x.id = 1 RETURN u").unwrap();
    let r = analyze(&q);
    let codes: Vec<_> = r.errors().map(|i| i.code).collect();
    assert!(
        codes.contains(&"unbound-variable"),
        "expected unbound-variable, got {codes:?}"
    );
}

#[test]
fn flags_unbound_variable_in_return() {
    let q = parse("MATCH (u) RETURN x.name").unwrap();
    let r = analyze(&q);
    assert!(r.has_errors());
}

#[test]
fn flags_unbound_variable_in_order_by() {
    let q = parse("MATCH (u) RETURN u ORDER BY x.created DESC").unwrap();
    let r = analyze(&q);
    assert!(r.has_errors());
}

#[test]
fn parameter_references_are_never_unbound() {
    let q = parse("MATCH (u) WHERE u.id = $uid RETURN u").unwrap();
    let r = analyze(&q);
    assert!(!r.has_errors());
}

#[test]
fn property_access_resolves_to_base_variable() {
    let q = parse("MATCH (u) WHERE u.profile.name = 'sd' RETURN u.profile.email").unwrap();
    let r = analyze(&q);
    assert!(!r.has_errors());
}

#[test]
fn list_literal_inner_variables_are_checked() {
    let q = parse("MATCH (u) WHERE u.id IN [u.id, x.id] RETURN u").unwrap();
    let r = analyze(&q);
    assert!(r.has_errors());
}

#[test]
fn permissive_schema_accepts_unknown_labels() {
    let q = parse("MATCH (u:NeverHeardOf) RETURN u").unwrap();
    let r = analyze(&q);
    assert!(!r.has_errors());
}

#[test]
fn custom_schema_rejects_unknown_label() {
    struct OnlyUser;
    impl Schema for OnlyUser {
        fn has_label(&self, label: &str) -> bool {
            label == "User"
        }
    }
    let q = parse("MATCH (u:User), (x:Robot) RETURN u").unwrap();
    let r = analyze_with(&q, &OnlyUser);
    let codes: Vec<_> = r.errors().map(|i| i.code).collect();
    assert!(
        codes.contains(&"unknown-label"),
        "expected unknown-label, got {codes:?}"
    );
}

#[test]
fn custom_schema_rejects_unknown_rel_type() {
    struct OnlyKnows;
    impl Schema for OnlyKnows {
        fn has_rel_type(&self, ty: &str) -> bool {
            ty == "KNOWS"
        }
    }
    let q = parse("MATCH (a)-[:FRENEMIES]->(b) RETURN a").unwrap();
    let r = analyze_with(&q, &OnlyKnows);
    let codes: Vec<_> = r.errors().map(|i| i.code).collect();
    assert!(codes.contains(&"unknown-rel-type"));
}

#[test]
fn no_errors_for_clean_query() {
    let q = parse(
        "MATCH (u:User)-[:FOLLOWS]->(f:User) \
         WHERE u.id = $uid AND f.role IN ['admin', 'owner'] \
         RETURN f.name AS name, f.score \
         ORDER BY f.score DESC \
         LIMIT 10",
    )
    .unwrap();
    let r = analyze(&q);
    assert!(!r.has_errors(), "expected clean, got {:?}", r.issues);
}

#[test]
fn nested_unbound_variable_inside_arithmetic() {
    let q = parse("MATCH (u) WHERE (u.x + y.z) > 10 RETURN u").unwrap();
    let r = analyze(&q);
    assert!(r.has_errors());
}

#[test]
fn report_distinguishes_errors_from_warnings() {
    let q = parse("MATCH (u) WHERE x.id = 1 RETURN u").unwrap();
    let r = analyze(&q);
    let err_count = r.errors().count();
    assert_eq!(err_count, 1);
}

// ---- UNWIND scope tests --------------------------------------------------

#[test]
fn unwind_self_shadow_alias_reports_unbound_error() {
    // The alias x is produced by this UNWIND clause; it must not be visible
    // while evaluating the list expression [x].
    let q = parse("UNWIND [x] AS x RETURN x").unwrap();
    let r = analyze(&q);
    let codes: Vec<_> = r.errors().map(|i| i.code).collect();
    assert!(
        codes.contains(&"unbound-variable"),
        "UNWIND [x] AS x should report unbound-variable for x in the list, got {codes:?}"
    );
}

#[test]
fn unwind_expr_sees_prior_match_binding() {
    // x is bound by MATCH before UNWIND; the list expression [x] is valid
    // even though UNWIND also produces x as its output alias.
    let q = parse("MATCH (x:Node) UNWIND [x] AS x RETURN x").unwrap();
    let r = analyze(&q);
    assert!(
        !r.has_errors(),
        "MATCH-bound x should be visible in UNWIND expr, got {:?}",
        r.issues
    );
}

#[test]
fn chained_unwind_second_sees_first_alias() {
    // The second UNWIND's expression references t, which is produced by the
    // first UNWIND. With ordered scoping, t must be in scope for the second
    // UNWIND's expression check.
    let q = parse("UNWIND $lists AS t UNWIND t AS item RETURN item").unwrap();
    let r = analyze(&q);
    assert!(
        !r.has_errors(),
        "second UNWIND should see alias from first UNWIND, got {:?}",
        r.issues
    );
}
