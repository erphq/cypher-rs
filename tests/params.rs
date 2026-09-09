//! Tests for parameter expressions (`$name`) not covered elsewhere.
//!
//! The grammar rule is: `param = ${ "$" ~ ident }` which maps to
//! `Expr::Param(String)` in the AST. The existing parse.rs covers
//! one case (param in WHERE equality). This file covers the remaining
//! positions where a param can legally appear.

use cypher_rs::*;

// --- helpers ---

fn parse_ok(src: &str) -> Query {
    parse(src).unwrap_or_else(|e| panic!("parse failed for: {src}\n  error: {e}"))
}

fn param_name(e: &Expr) -> &str {
    match e {
        Expr::Param(n) => n.as_str(),
        other => panic!("expected Param, got {other:?}"),
    }
}

// --- WHERE positions ---

#[test]
fn param_on_lhs_of_comparison() {
    let q = parse_ok("MATCH (u) WHERE $min <= u.age RETURN u");
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::Lte,
            lhs,
            ..
        }) => {
            assert_eq!(param_name(lhs), "min");
        }
        other => panic!("expected Lte, got {other:?}"),
    }
}

#[test]
fn param_in_boolean_conjunction() {
    let q = parse_ok("MATCH (u) WHERE u.age > $min AND u.age < $max RETURN u");
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::And,
            lhs,
            rhs,
        }) => {
            match lhs.as_ref() {
                Expr::Binary { rhs: l_rhs, .. } => assert_eq!(param_name(l_rhs), "min"),
                other => panic!("expected inner binary, got {other:?}"),
            }
            match rhs.as_ref() {
                Expr::Binary { rhs: r_rhs, .. } => assert_eq!(param_name(r_rhs), "max"),
                other => panic!("expected inner binary, got {other:?}"),
            }
        }
        other => panic!("expected AND, got {other:?}"),
    }
}

#[test]
fn param_in_in_expr() {
    let q = parse_ok("MATCH (u) WHERE u.role IN $roles RETURN u");
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::In, rhs, ..
        }) => {
            assert_eq!(param_name(rhs), "roles");
        }
        other => panic!("expected In with param, got {other:?}"),
    }
}

// --- RETURN positions ---

#[test]
fn param_as_standalone_return_item() {
    let q = parse_ok("MATCH (u) RETURN $val");
    match &q.clauses[1] {
        Clause::Return(r) => assert_eq!(param_name(&r.items[0].expr), "val"),
        _ => panic!("expected RETURN"),
    }
}

#[test]
fn param_in_return_arithmetic() {
    let q = parse_ok("MATCH (u) RETURN u.score + $bonus");
    match &q.clauses[1] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::Binary {
                op: BinOp::Add,
                rhs,
                ..
            } => assert_eq!(param_name(rhs), "bonus"),
            other => panic!("expected Add with param rhs, got {other:?}"),
        },
        _ => panic!("expected RETURN"),
    }
}

#[test]
fn param_in_list_literal() {
    let q = parse_ok("MATCH (u) WHERE u.id IN [$a, $b, $c] RETURN u");
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::In, rhs, ..
        }) => match rhs.as_ref() {
            Expr::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(param_name(&items[0]), "a");
                assert_eq!(param_name(&items[1]), "b");
                assert_eq!(param_name(&items[2]), "c");
            }
            other => panic!("expected List, got {other:?}"),
        },
        other => panic!("expected In, got {other:?}"),
    }
}

// --- LIMIT / SKIP positions ---

#[test]
fn param_in_limit() {
    let q = parse_ok("MATCH (u) RETURN u LIMIT $page_size");
    match &q.clauses[2] {
        Clause::Limit(e) => assert_eq!(param_name(e), "page_size"),
        other => panic!("expected LIMIT with param, got {other:?}"),
    }
}

#[test]
fn param_in_skip() {
    let q = parse_ok("MATCH (u) RETURN u SKIP $offset");
    match &q.clauses[2] {
        Clause::Skip(e) => assert_eq!(param_name(e), "offset"),
        other => panic!("expected SKIP with param, got {other:?}"),
    }
}

#[test]
fn param_in_limit_and_skip_together() {
    let q = parse_ok("MATCH (u) RETURN u LIMIT $lim SKIP $off");
    match &q.clauses[2] {
        Clause::Limit(e) => assert_eq!(param_name(e), "lim"),
        other => panic!("expected LIMIT, got {other:?}"),
    }
    match &q.clauses[3] {
        Clause::Skip(e) => assert_eq!(param_name(e), "off"),
        other => panic!("expected SKIP, got {other:?}"),
    }
}

// --- ORDER BY positions ---

#[test]
fn param_in_order_by_expr() {
    let q = parse_ok("MATCH (u) RETURN u ORDER BY $sort_key ASC");
    match &q.clauses[2] {
        Clause::OrderBy(items) => {
            assert_eq!(items.len(), 1);
            assert_eq!(param_name(&items[0].expr), "sort_key");
            assert!(!items[0].desc);
        }
        other => panic!("expected ORDER BY with param, got {other:?}"),
    }
}

// --- name preservation ---

#[test]
fn param_name_with_underscore_preserved() {
    let q = parse_ok("MATCH (u) WHERE u.user_id = $user_id RETURN u");
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { rhs, .. }) => assert_eq!(param_name(rhs), "user_id"),
        other => panic!("expected WHERE binary, got {other:?}"),
    }
}

#[test]
fn multiple_distinct_params_same_query() {
    let q = parse_ok("MATCH (u) WHERE u.age > $min AND u.age < $max RETURN u LIMIT $n SKIP $off");
    // We parsed successfully and clause count is correct.
    assert_eq!(q.clauses.len(), 5);
    // Spot-check LIMIT and SKIP carry their own param names.
    match &q.clauses[3] {
        Clause::Limit(e) => assert_eq!(param_name(e), "n"),
        other => panic!("expected LIMIT, got {other:?}"),
    }
    match &q.clauses[4] {
        Clause::Skip(e) => assert_eq!(param_name(e), "off"),
        other => panic!("expected SKIP, got {other:?}"),
    }
}
