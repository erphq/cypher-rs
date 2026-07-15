//! Tests for the DELETE / DETACH DELETE clause (v0.15).

use cypher_rs::*;

#[test]
fn parses_simple_delete() {
    let q = parse("MATCH (n) DELETE n").unwrap();
    assert_eq!(q.clauses.len(), 2);
    match &q.clauses[1] {
        Clause::Delete { detach, exprs } => {
            assert!(!detach);
            assert_eq!(exprs.len(), 1);
            assert!(matches!(&exprs[0], Expr::Variable(v) if v == "n"));
        }
        other => panic!("expected Delete, got {other:?}"),
    }
}

#[test]
fn parses_detach_delete_sets_flag() {
    let q = parse("MATCH (n) DETACH DELETE n").unwrap();
    match &q.clauses[1] {
        Clause::Delete { detach, .. } => assert!(detach),
        other => panic!("expected Delete, got {other:?}"),
    }
}

#[test]
fn parses_delete_multiple_exprs() {
    let q = parse("MATCH (a)-[r]->(b) DELETE r, a, b").unwrap();
    match &q.clauses[1] {
        Clause::Delete { detach, exprs } => {
            assert!(!detach);
            assert_eq!(exprs.len(), 3);
        }
        other => panic!("expected Delete, got {other:?}"),
    }
}

#[test]
fn parses_delete_case_insensitive() {
    assert!(parse("MATCH (n) delete n").is_ok());
    assert!(parse("MATCH (n) detach delete n").is_ok());
}

#[test]
fn delete_and_detach_are_reserved_keywords() {
    assert!(parse("MATCH (delete) RETURN delete").is_err());
    assert!(parse("MATCH (detach) RETURN detach").is_err());
}

#[test]
fn delete_bound_variable_passes_sema() {
    let q = parse("MATCH (n) DELETE n").unwrap();
    let report = analyze(&q);
    assert!(
        !report.has_errors(),
        "unexpected errors: {:?}",
        report.issues
    );
}

#[test]
fn delete_unbound_variable_is_an_error() {
    let q = parse("MATCH (n) DELETE x").unwrap();
    let report = analyze(&q);
    assert!(report.has_errors());
    assert!(report.issues.iter().any(|i| i.code == "unbound-variable"));
}

#[test]
fn delete_lowers_to_delete_node_in_plan() {
    let q = parse("MATCH (n) DELETE n").unwrap();
    let p = plan(&q).unwrap();
    let rendered = p.to_string();
    assert!(rendered.contains("Delete"), "plan: {rendered}");
    assert!(rendered.contains("detach: false"), "plan: {rendered}");
    assert!(rendered.contains("Scan"), "plan: {rendered}");
}

#[test]
fn detach_delete_sets_detach_true_in_plan() {
    let q = parse("MATCH (n) DETACH DELETE n").unwrap();
    let p = plan(&q).unwrap();
    assert!(p.to_string().contains("detach: true"));
}

#[test]
fn filter_pushes_into_delete_input() {
    let q = parse("MATCH (n:User) WHERE n.active = true DELETE n").unwrap();
    let p = plan(&q).unwrap();
    let optimized = optimize(p);
    let rendered = optimized.to_string();
    assert!(rendered.contains("Delete"), "plan: {rendered}");
    assert!(rendered.contains("Filter"), "plan: {rendered}");
}
