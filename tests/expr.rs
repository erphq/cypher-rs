//! Tests for expression operators not covered elsewhere.
//! Covers: OR, Neq/Lte/Gte/Lt comparisons, Div/Mod/Sub arithmetic,
//! unary negation, NOT over a binary, and double-quoted string literals.

use cypher_rs::*;

#[test]
fn parses_or_in_where() {
    let q = parse("MATCH (u) WHERE u.role = 'admin' OR u.role = 'owner' RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::Or, .. }) => {}
        other => panic!("expected OR at top level, got {other:?}"),
    }
}

#[test]
fn and_binds_tighter_than_or() {
    // a OR b AND c  parses as  a OR (b AND c)
    let q = parse("MATCH (u) WHERE u.x = 1 OR u.y = 2 AND u.z = 3 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::Or, rhs, ..
        }) => {
            assert!(
                matches!(rhs.as_ref(), Expr::Binary { op: BinOp::And, .. }),
                "expected rhs of OR to be AND, got {rhs:?}"
            );
        }
        other => panic!("expected OR at top level, got {other:?}"),
    }
}

#[test]
fn parses_chained_or() {
    let q = parse("MATCH (u) WHERE u.a = 1 OR u.b = 2 OR u.c = 3 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::Or, .. }) => {}
        other => panic!("expected OR, got {other:?}"),
    }
}

#[test]
fn parses_neq_operator() {
    let q = parse("MATCH (u) WHERE u.status <> 'deleted' RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::Neq, .. }) => {}
        other => panic!("expected <>, got {other:?}"),
    }
}

#[test]
fn parses_lte_operator() {
    let q = parse("MATCH (u) WHERE u.age <= 18 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::Lte, .. }) => {}
        other => panic!("expected <=, got {other:?}"),
    }
}

#[test]
fn parses_gte_operator() {
    let q = parse("MATCH (u) WHERE u.score >= 100 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::Gte, .. }) => {}
        other => panic!("expected >=, got {other:?}"),
    }
}

#[test]
fn parses_lt_operator() {
    let q = parse("MATCH (u) WHERE u.age < 18 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::Lt, .. }) => {}
        other => panic!("expected <, got {other:?}"),
    }
}

#[test]
fn parses_division() {
    let q = parse("RETURN 10 / 2").unwrap();
    match &q.clauses[0] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::Binary {
                op: BinOp::Div,
                lhs,
                rhs,
            } => {
                assert!(matches!(lhs.as_ref(), Expr::Literal(Literal::Int(10))));
                assert!(matches!(rhs.as_ref(), Expr::Literal(Literal::Int(2))));
            }
            other => panic!("expected Div, got {other:?}"),
        },
        _ => panic!("expected RETURN"),
    }
}

#[test]
fn parses_modulo() {
    let q = parse("RETURN 7 % 3").unwrap();
    match &q.clauses[0] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::Binary { op: BinOp::Mod, .. } => {}
            other => panic!("expected Mod, got {other:?}"),
        },
        _ => panic!("expected RETURN"),
    }
}

#[test]
fn parses_subtraction() {
    let q = parse("RETURN 5 - 3").unwrap();
    match &q.clauses[0] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::Binary {
                op: BinOp::Sub,
                lhs,
                rhs,
            } => {
                assert!(matches!(lhs.as_ref(), Expr::Literal(Literal::Int(5))));
                assert!(matches!(rhs.as_ref(), Expr::Literal(Literal::Int(3))));
            }
            other => panic!("expected Sub, got {other:?}"),
        },
        _ => panic!("expected RETURN"),
    }
}

#[test]
fn parses_unary_negation() {
    let q = parse("RETURN -1").unwrap();
    match &q.clauses[0] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::Unary {
                op: UnOp::Neg,
                operand,
            } => {
                assert!(matches!(operand.as_ref(), Expr::Literal(Literal::Int(1))));
            }
            other => panic!("expected Neg, got {other:?}"),
        },
        _ => panic!("expected RETURN"),
    }
}

#[test]
fn parses_unary_negation_of_property() {
    let q = parse("MATCH (u) RETURN -u.score").unwrap();
    match &q.clauses[1] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::Unary {
                op: UnOp::Neg,
                operand,
            } => {
                assert!(matches!(operand.as_ref(), Expr::Property { key, .. } if key == "score"));
            }
            other => panic!("expected Neg over property, got {other:?}"),
        },
        _ => panic!("expected RETURN"),
    }
}

#[test]
fn parses_not_over_comparison() {
    let q = parse("MATCH (u) WHERE NOT u.age > 18 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Unary {
            op: UnOp::Not,
            operand,
        }) => {
            assert!(matches!(
                operand.as_ref(),
                Expr::Binary { op: BinOp::Gt, .. }
            ));
        }
        other => panic!("expected NOT over Gt, got {other:?}"),
    }
}

#[test]
fn parses_double_quoted_string() {
    let q = parse(r#"RETURN "hello""#).unwrap();
    match &q.clauses[0] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::Literal(Literal::String(s)) => assert_eq!(s, "hello"),
            other => panic!("expected String, got {other:?}"),
        },
        _ => panic!("expected RETURN"),
    }
}

#[test]
fn parses_mixed_operator_precedence_in_filter() {
    // NOT (a AND b) OR c: NOT binds tightest, then AND, then OR
    // Written as: NOT u.x AND u.y OR u.z
    // Should parse as: (NOT u.x AND u.y) OR u.z
    //   => OR( AND(NOT(u.x), u.y), u.z )
    let q =
        parse("MATCH (u) WHERE NOT u.active AND u.verified OR u.admin = true RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::Or, .. }) => {}
        other => panic!("expected OR at top level, got {other:?}"),
    }
}

#[test]
fn parses_all_comparison_ops_round_trip() {
    for (op_str, expected) in [
        ("=", BinOp::Eq),
        ("<>", BinOp::Neq),
        ("<", BinOp::Lt),
        ("<=", BinOp::Lte),
        (">", BinOp::Gt),
        (">=", BinOp::Gte),
    ] {
        let src = format!("MATCH (u) WHERE u.x {op_str} 1 RETURN u");
        let q = parse(&src).unwrap();
        match &q.clauses[1] {
            Clause::Where(Expr::Binary { op, .. }) => {
                assert_eq!(*op, expected, "wrong op for {op_str}");
            }
            other => panic!("expected Binary for {op_str}, got {other:?}"),
        }
    }
}
