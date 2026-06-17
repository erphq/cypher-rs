//! Tests for STARTS WITH, ENDS WITH, and CONTAINS string predicates.
//! These are part of the openCypher spec and help reach TCK conformance.

use cypher_rs::*;

#[test]
fn parses_starts_with() {
    let q =
        parse("MATCH (u:User) WHERE u.name STARTS WITH 'Alice' RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::StartsWith,
            lhs,
            rhs,
        }) => {
            assert!(matches!(lhs.as_ref(), Expr::Property { key, .. } if key == "name"));
            assert!(
                matches!(rhs.as_ref(), Expr::Literal(Literal::String(s)) if s == "Alice")
            );
        }
        other => panic!("expected STARTS WITH, got {other:?}"),
    }
}

#[test]
fn parses_ends_with() {
    let q =
        parse("MATCH (u:User) WHERE u.email ENDS WITH '@example.com' RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::EndsWith,
            lhs,
            rhs,
        }) => {
            assert!(matches!(lhs.as_ref(), Expr::Property { key, .. } if key == "email"));
            assert!(
                matches!(rhs.as_ref(), Expr::Literal(Literal::String(s)) if s == "@example.com")
            );
        }
        other => panic!("expected ENDS WITH, got {other:?}"),
    }
}

#[test]
fn parses_contains() {
    let q = parse("MATCH (p:Post) WHERE p.body CONTAINS 'keyword' RETURN p").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::Contains,
            lhs,
            rhs,
        }) => {
            assert!(matches!(lhs.as_ref(), Expr::Property { key, .. } if key == "body"));
            assert!(
                matches!(rhs.as_ref(), Expr::Literal(Literal::String(s)) if s == "keyword")
            );
        }
        other => panic!("expected CONTAINS, got {other:?}"),
    }
}

#[test]
fn starts_with_keyword_is_case_insensitive() {
    let q = parse("MATCH (u) WHERE u.name starts with 'A' RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::StartsWith,
            ..
        }) => {}
        other => panic!("expected STARTS WITH (case-insensitive), got {other:?}"),
    }
}

#[test]
fn ends_with_keyword_is_case_insensitive() {
    let q = parse("MATCH (u) WHERE u.name ends with '.com' RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::EndsWith,
            ..
        }) => {}
        other => panic!("expected ENDS WITH (case-insensitive), got {other:?}"),
    }
}

#[test]
fn contains_keyword_is_case_insensitive() {
    let q = parse("MATCH (u) WHERE u.bio contains 'rust' RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::Contains,
            ..
        }) => {}
        other => panic!("expected CONTAINS (case-insensitive), got {other:?}"),
    }
}

#[test]
fn starts_with_combined_with_and() {
    let q = parse(
        "MATCH (u:User) WHERE u.name STARTS WITH 'Alice' AND u.age > 18 RETURN u",
    )
    .unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::And,
            lhs,
            ..
        }) => {
            assert!(
                matches!(lhs.as_ref(), Expr::Binary { op: BinOp::StartsWith, .. }),
                "expected STARTS WITH as lhs of AND, got {lhs:?}"
            );
        }
        other => panic!("expected AND at top level, got {other:?}"),
    }
}

#[test]
fn contains_combined_with_or() {
    let q = parse(
        "MATCH (p:Post) WHERE p.title CONTAINS 'Rust' OR p.title CONTAINS 'Cargo' RETURN p",
    )
    .unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::Or,
            lhs,
            rhs,
        }) => {
            assert!(matches!(lhs.as_ref(), Expr::Binary { op: BinOp::Contains, .. }));
            assert!(matches!(rhs.as_ref(), Expr::Binary { op: BinOp::Contains, .. }));
        }
        other => panic!("expected OR at top level, got {other:?}"),
    }
}

#[test]
fn string_predicate_rhs_can_be_param() {
    let q = parse("MATCH (u) WHERE u.name STARTS WITH $prefix RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::StartsWith,
            rhs,
            ..
        }) => {
            assert!(
                matches!(rhs.as_ref(), Expr::Param(p) if p == "prefix"),
                "expected Param(prefix) as rhs, got {rhs:?}"
            );
        }
        other => panic!("expected STARTS WITH with param rhs, got {other:?}"),
    }
}

#[test]
fn string_predicate_in_full_pipeline_produces_filter() {
    let q =
        parse("MATCH (u:User) WHERE u.email ENDS WITH '@corp.com' RETURN u.name LIMIT 10")
            .unwrap();
    let p = plan(&q).unwrap();
    let s = format!("{p}");
    assert!(s.contains("Limit"), "expected Limit in plan: {s}");
    assert!(s.contains("Project"), "expected Project in plan: {s}");
    assert!(s.contains("Filter"), "expected Filter in plan: {s}");
    assert!(s.contains("Scan"), "expected Scan in plan: {s}");
}

#[test]
fn not_starts_with() {
    let q = parse("MATCH (u) WHERE NOT u.name STARTS WITH 'Bot' RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Unary {
            op: UnOp::Not,
            operand,
        }) => {
            assert!(
                matches!(operand.as_ref(), Expr::Binary { op: BinOp::StartsWith, .. }),
                "expected NOT wrapping STARTS WITH, got {operand:?}"
            );
        }
        other => panic!("expected NOT STARTS WITH, got {other:?}"),
    }
}
