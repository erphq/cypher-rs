//! v0.13 UNWIND clause tests.
//!
//! UNWIND turns a list expression into one row per element, binding
//! each element to a variable. Equivalent to SQL UNNEST / lateral apply.

use std::collections::HashSet;

use cypher_rs::*;

// ---- parse tests ---------------------------------------------------------

#[test]
fn parses_unwind_list_literal() {
    let q = parse("UNWIND [1, 2, 3] AS x RETURN x").unwrap();
    assert!(
        matches!(&q.clauses[0], Clause::Unwind { .. }),
        "expected Clause::Unwind, got {:?}",
        q.clauses[0]
    );
    match &q.clauses[0] {
        Clause::Unwind { expr, var } => {
            assert_eq!(var, "x");
            assert!(
                matches!(expr, Expr::List(items) if items.len() == 3),
                "expected 3-element list, got {expr:?}"
            );
        }
        _ => unreachable!(),
    }
}

#[test]
fn parses_unwind_variable_expression() {
    let q = parse("MATCH (u:User) UNWIND u.tags AS tag RETURN tag").unwrap();
    match &q.clauses[1] {
        Clause::Unwind { expr, var } => {
            assert_eq!(var, "tag");
            assert!(
                matches!(expr, Expr::Property { key, .. } if key == "tags"),
                "expected property access, got {expr:?}"
            );
        }
        other => panic!("expected Clause::Unwind, got {other:?}"),
    }
}

#[test]
fn parses_unwind_keyword_case_insensitive() {
    let q = parse("unwind [1, 2] as n RETURN n").unwrap();
    assert!(
        matches!(&q.clauses[0], Clause::Unwind { .. }),
        "expected Clause::Unwind from lowercase keyword"
    );
}

#[test]
fn parses_unwind_with_param_list() {
    let q = parse("UNWIND $ids AS id RETURN id").unwrap();
    match &q.clauses[0] {
        Clause::Unwind { expr, var } => {
            assert_eq!(var, "id");
            assert!(
                matches!(expr, Expr::Param(p) if p == "ids"),
                "expected Param(ids), got {expr:?}"
            );
        }
        other => panic!("expected Clause::Unwind, got {other:?}"),
    }
}

#[test]
fn unwind_keyword_not_usable_as_identifier() {
    assert!(
        parse("MATCH (unwind:Foo) RETURN unwind").is_err(),
        "UNWIND must not parse as an identifier"
    );
}

// ---- plan tests ----------------------------------------------------------

#[test]
fn plan_unwind_standalone_contains_unwind_node() {
    let q = parse("UNWIND [1, 2, 3] AS x RETURN x").unwrap();
    let p = plan(&q).unwrap();
    let s = format!("{p}");
    assert!(s.contains("Unwind"), "expected Unwind in plan:\n{s}");
    assert!(s.contains("Project"), "expected Project in plan:\n{s}");
}

#[test]
fn plan_unwind_after_match_chains_correctly() {
    let q = parse("MATCH (u:User) UNWIND u.scores AS s RETURN s").unwrap();
    let p = plan(&q).unwrap();
    let s = format!("{p}");
    assert!(s.contains("Unwind"), "expected Unwind in plan:\n{s}");
    assert!(s.contains("Scan"), "expected Scan in plan:\n{s}");
}

#[test]
fn plan_unwind_var_in_display() {
    let q = parse("UNWIND [10, 20] AS item RETURN item").unwrap();
    let p = plan(&q).unwrap();
    let s = format!("{p}");
    assert!(
        s.contains("var: item"),
        "expected 'var: item' in plan display:\n{s}"
    );
}

// ---- cost tests ----------------------------------------------------------

#[test]
fn unwind_cost_multiplies_by_list_length() {
    let q = parse("UNWIND [1, 2, 3, 4, 5] AS x RETURN x").unwrap();
    let p = plan(&q).unwrap();
    let m = CardinalityCostModel::default();
    let est = estimate(&p, &m);
    // Empty input has cardinality 1; 5 elements; Project passes through.
    // Unwind: 1 * 5 = 5; Project: same cardinality.
    assert!(
        est.cardinality >= 5.0 && est.cardinality <= 6.0,
        "expected ~5 rows from 5-element UNWIND, got {est:?}"
    );
}

#[test]
fn unwind_cost_uses_default_for_dynamic_list() {
    let q = parse("UNWIND $ids AS id RETURN id").unwrap();
    let p = plan(&q).unwrap();
    let m = CardinalityCostModel::default();
    let est = estimate(&p, &m);
    // Dynamic list: default multiplier of 10; Empty input cardinality 1.
    assert!(
        est.cardinality >= 10.0,
        "expected default 10x multiplier for param list, got {est:?}"
    );
}

// ---- prune tests ---------------------------------------------------------

fn s(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn output_columns_unwind_includes_var() {
    let q = parse("UNWIND [1, 2] AS x RETURN x").unwrap();
    let p = plan(&q).unwrap();
    let cols = output_columns(&p);
    assert!(
        cols.contains("x"),
        "expected 'x' in output columns: {cols:?}"
    );
}

#[test]
fn output_columns_unwind_after_match_includes_both() {
    let q = parse("MATCH (u:User) UNWIND u.tags AS t RETURN u, t").unwrap();
    let p = plan(&q).unwrap();
    let cols = output_columns(&p);
    assert!(cols.contains("u"), "expected 'u': {cols:?}");
    assert!(cols.contains("t"), "expected 't': {cols:?}");
}

#[test]
fn required_input_columns_unwind_excludes_var_from_demand() {
    let q = parse("UNWIND [1, 2] AS x RETURN x").unwrap();
    let p = plan(&q).unwrap();
    // Walk into the Unwind node (the Project's input).
    if let Plan::Project { input, .. } = &p {
        let demand = required_input_columns(input, &s(&["x"]));
        assert!(
            !demand.contains("x"),
            "Unwind produces 'x'; its input should not be asked for it: {demand:?}"
        );
    } else {
        panic!("expected Project at root");
    }
}

#[test]
fn required_input_columns_unwind_expr_vars_added_to_demand() {
    let q = parse("MATCH (u:User) UNWIND u.tags AS t RETURN t").unwrap();
    let p = plan(&q).unwrap();
    // Walk to Unwind node.
    if let Plan::Project { input, .. } = &p {
        let demand = required_input_columns(input, &s(&["t"]));
        // Unwind expression is u.tags; the input must supply 'u'.
        assert!(
            demand.contains("u"),
            "Unwind expr references 'u'; demand should include it: {demand:?}"
        );
        assert!(
            !demand.contains("t"),
            "Unwind produces 't'; not required of input: {demand:?}"
        );
    } else {
        panic!("expected Project at root");
    }
}
