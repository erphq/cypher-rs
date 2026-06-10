//! Tests for IS NULL and IS NOT NULL predicates.
//!
//! Both operators are wired through grammar, parser, AST
//! (UnOp::IsNull / UnOp::IsNotNull), sema, and optimizer but had no
//! dedicated test coverage before this file.

use cypher_rs::*;

// ---- parsing ------------------------------------------------------------

#[test]
fn parses_is_null_on_property() {
    let q = parse("MATCH (u) WHERE u.email IS NULL RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Unary {
            op: UnOp::IsNull,
            operand,
        }) => {
            assert!(matches!(
                operand.as_ref(),
                Expr::Property { key, .. } if key == "email"
            ));
        }
        other => panic!("expected IS NULL, got {other:?}"),
    }
}

#[test]
fn parses_is_not_null_on_property() {
    let q = parse("MATCH (u) WHERE u.email IS NOT NULL RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Unary {
            op: UnOp::IsNotNull,
            operand,
        }) => {
            assert!(matches!(
                operand.as_ref(),
                Expr::Property { key, .. } if key == "email"
            ));
        }
        other => panic!("expected IS NOT NULL, got {other:?}"),
    }
}

#[test]
fn parses_is_null_on_bare_variable() {
    let q = parse("MATCH (u) WHERE u IS NULL RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Unary {
            op: UnOp::IsNull,
            operand,
        }) => {
            assert!(matches!(operand.as_ref(), Expr::Variable(v) if v == "u"));
        }
        other => panic!("expected IS NULL on variable, got {other:?}"),
    }
}

#[test]
fn parses_is_not_null_on_bare_variable() {
    let q = parse("MATCH (u) WHERE u IS NOT NULL RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Unary {
            op: UnOp::IsNotNull,
            operand,
        }) => {
            assert!(matches!(operand.as_ref(), Expr::Variable(v) if v == "u"));
        }
        other => panic!("expected IS NOT NULL on variable, got {other:?}"),
    }
}

#[test]
fn is_null_case_insensitive() {
    let q = parse("MATCH (u) WHERE u.name is null RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Unary { op: UnOp::IsNull, .. }) => {}
        other => panic!("expected IS NULL (lowercase), got {other:?}"),
    }
}

#[test]
fn is_not_null_case_insensitive() {
    let q = parse("MATCH (u) WHERE u.name is not null RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Unary { op: UnOp::IsNotNull, .. }) => {}
        other => panic!("expected IS NOT NULL (lowercase), got {other:?}"),
    }
}

#[test]
fn is_null_combined_with_and() {
    // IS NULL binds tighter than AND (it is a postfix tail on cmp_expr).
    let q =
        parse("MATCH (u) WHERE u.active = true AND u.email IS NULL RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::And,
            rhs,
            ..
        }) => {
            assert!(
                matches!(rhs.as_ref(), Expr::Unary { op: UnOp::IsNull, .. }),
                "expected IS NULL as rhs of AND, got {rhs:?}"
            );
        }
        other => panic!("expected AND at top level, got {other:?}"),
    }
}

#[test]
fn is_not_null_combined_with_or() {
    let q =
        parse("MATCH (u) WHERE u.email IS NOT NULL OR u.phone IS NOT NULL RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::Or, .. }) => {}
        other => panic!("expected OR combining two IS NOT NULL predicates, got {other:?}"),
    }
}

// ---- semantic analysis --------------------------------------------------

#[test]
fn sema_accepts_is_null_on_bound_variable() {
    let q = parse("MATCH (u) WHERE u.email IS NULL RETURN u").unwrap();
    let r = analyze(&q);
    assert!(!r.has_errors(), "unexpected errors: {:?}", r.issues);
}

#[test]
fn sema_accepts_is_not_null_on_bound_variable() {
    let q = parse("MATCH (u) WHERE u.name IS NOT NULL RETURN u").unwrap();
    let r = analyze(&q);
    assert!(!r.has_errors(), "unexpected errors: {:?}", r.issues);
}

#[test]
fn sema_flags_unbound_variable_inside_is_null() {
    let q = parse("MATCH (u) WHERE x.email IS NULL RETURN u").unwrap();
    let r = analyze(&q);
    let codes: Vec<_> = r.errors().map(|i| i.code).collect();
    assert!(
        codes.contains(&"unbound-variable"),
        "expected unbound-variable error, got {codes:?}"
    );
}

#[test]
fn sema_flags_unbound_variable_inside_is_not_null() {
    let q = parse("MATCH (u) WHERE z IS NOT NULL RETURN u").unwrap();
    let r = analyze(&q);
    assert!(
        r.has_errors(),
        "expected an error for unbound variable in IS NOT NULL"
    );
}

// ---- planning -----------------------------------------------------------

#[test]
fn planner_produces_filter_for_is_null() {
    let q = parse("MATCH (u:User) WHERE u.email IS NULL RETURN u").unwrap();
    let p = plan(&q).unwrap();
    let s = p.to_string();
    assert!(s.contains("Filter"), "expected Filter node in plan:\n{s}");
}

#[test]
fn planner_produces_filter_for_is_not_null() {
    let q = parse("MATCH (u:User) WHERE u.email IS NOT NULL RETURN u").unwrap();
    let p = plan(&q).unwrap();
    let s = p.to_string();
    assert!(s.contains("Filter"), "expected Filter node in plan:\n{s}");
}

// ---- optimizer pushdown -------------------------------------------------

#[test]
fn optimizer_pushes_is_null_filter_through_expand() {
    // Predicate references `u` (the src). Expand does not introduce `u`,
    // so the filter pushes below the Expand.
    let q =
        parse("MATCH (u:User)-[:FOLLOWS]->(f:User) WHERE u.email IS NULL RETURN f").unwrap();
    let raw = plan(&q).unwrap();
    let p = optimize(raw);
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Expand { input: ei, .. } => {
                assert!(
                    matches!(*ei, Plan::Filter { .. }),
                    "expected IS NULL filter pushed below Expand, got {ei:?}"
                );
            }
            other => panic!("expected Expand under Project, got {other:?}"),
        },
        other => panic!("expected Project root, got {other:?}"),
    }
}

#[test]
fn optimizer_keeps_is_null_filter_above_expand_for_dst_var() {
    // Predicate references `f` (the dst). Expand introduces `f`,
    // so the filter must stay above the Expand.
    let q =
        parse("MATCH (u:User)-[:FOLLOWS]->(f:User) WHERE f.email IS NULL RETURN f").unwrap();
    let raw = plan(&q).unwrap();
    let p = optimize(raw);
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Filter { input: i2, .. } => {
                assert!(
                    matches!(*i2, Plan::Expand { .. }),
                    "expected Filter staying above Expand, got {i2:?}"
                );
            }
            other => panic!("expected Filter over Expand, got {other:?}"),
        },
        other => panic!("expected Project root, got {other:?}"),
    }
}

#[test]
fn optimizer_pushes_is_null_into_correct_cartesian_side() {
    // Predicate references `u` only, which is bound by the left Scan.
    // The filter should push into the left side of the Cartesian.
    let q =
        parse("MATCH (u:User), (v:Post) WHERE u.email IS NULL RETURN u, v").unwrap();
    let raw = plan(&q).unwrap();
    let p = optimize(raw);
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Cartesian { left, right } => {
                assert!(
                    matches!(*left, Plan::Filter { .. }),
                    "expected IS NULL filter pushed into left Cartesian side, got {left:?}"
                );
                assert!(
                    matches!(*right, Plan::Scan { .. }),
                    "expected bare Scan on right Cartesian side, got {right:?}"
                );
            }
            other => panic!("expected Cartesian under Project, got {other:?}"),
        },
        other => panic!("expected Project root, got {other:?}"),
    }
}
