//! Tests for the XOR boolean operator (openCypher 9 section 7.1).
//! XOR has lower precedence than AND and higher precedence than OR:
//!   NOT > AND > XOR > OR

use cypher_rs::*;

#[test]
fn parses_simple_xor() {
    let q = parse("MATCH (u) WHERE u.a = 1 XOR u.b = 2 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::Xor, .. }) => {}
        other => panic!("expected XOR, got {other:?}"),
    }
}

#[test]
fn xor_lowercase_accepted() {
    let q = parse("MATCH (u) WHERE u.a = 1 xor u.b = 2 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::Xor, .. }) => {}
        other => panic!("expected XOR (lowercase), got {other:?}"),
    }
}

#[test]
fn xor_mixed_case_accepted() {
    let q = parse("MATCH (u) WHERE u.a = 1 Xor u.b = 2 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::Xor, .. }) => {}
        other => panic!("expected XOR (mixed case), got {other:?}"),
    }
}

#[test]
fn xor_is_left_associative() {
    // a XOR b XOR c parses as (a XOR b) XOR c
    let q = parse("MATCH (u) WHERE u.a = 1 XOR u.b = 2 XOR u.c = 3 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::Xor,
            lhs,
            ..
        }) => {
            assert!(
                matches!(lhs.as_ref(), Expr::Binary { op: BinOp::Xor, .. }),
                "lhs of outermost XOR should itself be XOR (left-assoc), got {lhs:?}"
            );
        }
        other => panic!("expected XOR at top level, got {other:?}"),
    }
}

#[test]
fn xor_precedence_above_or() {
    // a OR b XOR c parses as a OR (b XOR c)
    let q = parse("MATCH (u) WHERE u.a = 1 OR u.b = 2 XOR u.c = 3 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::Or, rhs, ..
        }) => {
            assert!(
                matches!(rhs.as_ref(), Expr::Binary { op: BinOp::Xor, .. }),
                "rhs of OR should be XOR, got {rhs:?}"
            );
        }
        other => panic!("expected OR at top level, got {other:?}"),
    }
}

#[test]
fn and_precedence_above_xor() {
    // a XOR b AND c parses as a XOR (b AND c)
    let q = parse("MATCH (u) WHERE u.a = 1 XOR u.b = 2 AND u.c = 3 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::Xor,
            rhs,
            ..
        }) => {
            assert!(
                matches!(rhs.as_ref(), Expr::Binary { op: BinOp::And, .. }),
                "rhs of XOR should be AND, got {rhs:?}"
            );
        }
        other => panic!("expected XOR at top level, got {other:?}"),
    }
}

#[test]
fn not_precedence_above_xor() {
    // NOT a XOR b parses as (NOT a) XOR b
    let q = parse("MATCH (u) WHERE NOT u.active XOR u.verified RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::Xor,
            lhs,
            ..
        }) => {
            assert!(
                matches!(lhs.as_ref(), Expr::Unary { op: UnOp::Not, .. }),
                "lhs of XOR should be NOT, got {lhs:?}"
            );
        }
        other => panic!("expected XOR at top level, got {other:?}"),
    }
}

#[test]
fn xor_is_reserved_keyword() {
    // A variable named `xor` should fail to parse.
    assert!(
        parse("MATCH (xor) RETURN xor").is_err(),
        "xor should be a reserved keyword"
    );
}

#[test]
fn xor_in_return_expression() {
    let q = parse("RETURN true XOR false").unwrap();
    match &q.clauses[0] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::Binary { op: BinOp::Xor, .. } => {}
            other => panic!("expected XOR in RETURN, got {other:?}"),
        },
        other => panic!("expected RETURN, got {other:?}"),
    }
}

#[test]
fn xor_with_sema_passes() {
    let q = parse("MATCH (u:User) WHERE u.a = 1 XOR u.b = 2 RETURN u").unwrap();
    let report = analyze(&q);
    assert!(
        !report.has_errors(),
        "unexpected sema errors: {:?}",
        report.issues
    );
}
