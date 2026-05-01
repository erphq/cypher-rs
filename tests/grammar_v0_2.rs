//! v0.2 grammar growth: ORDER BY · OPTIONAL MATCH · list literals · IN.

use cypher_rs::*;

#[test]
fn parses_optional_match() {
    let q = parse("OPTIONAL MATCH (u:User) RETURN u").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            assert!(m.optional, "expected OPTIONAL flag set");
            assert_eq!(m.patterns[0].anchor.var.as_deref(), Some("u"));
        }
        _ => panic!("expected MATCH"),
    }
}

#[test]
fn match_without_optional_keeps_flag_false() {
    let q = parse("MATCH (u:User) RETURN u").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => assert!(!m.optional),
        _ => panic!("expected MATCH"),
    }
}

#[test]
fn parses_order_by_default_asc() {
    let q = parse("MATCH (u) RETURN u.name ORDER BY u.name").unwrap();
    match &q.clauses[2] {
        Clause::OrderBy(items) => {
            assert_eq!(items.len(), 1);
            assert!(!items[0].desc);
        }
        c => panic!("expected ORDER BY, got {c:?}"),
    }
}

#[test]
fn parses_order_by_desc() {
    let q = parse("MATCH (u) RETURN u.created ORDER BY u.created DESC").unwrap();
    match &q.clauses[2] {
        Clause::OrderBy(items) => {
            assert_eq!(items.len(), 1);
            assert!(items[0].desc);
        }
        c => panic!("expected ORDER BY, got {c:?}"),
    }
}

#[test]
fn parses_order_by_multiple_keys_mixed() {
    let q = parse("MATCH (u) RETURN u ORDER BY u.priority DESC, u.name ASC").unwrap();
    match &q.clauses[2] {
        Clause::OrderBy(items) => {
            assert_eq!(items.len(), 2);
            assert!(items[0].desc);
            assert!(!items[1].desc);
        }
        c => panic!("expected ORDER BY, got {c:?}"),
    }
}

#[test]
fn parses_order_by_with_limit() {
    let q = parse("MATCH (u) RETURN u ORDER BY u.score DESC LIMIT 10").unwrap();
    assert_eq!(q.clauses.len(), 4);
    assert!(matches!(q.clauses[2], Clause::OrderBy(_)));
    assert!(matches!(q.clauses[3], Clause::Limit(_)));
}

#[test]
fn parses_empty_list_literal() {
    let q = parse("RETURN []").unwrap();
    match &q.clauses[0] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::List(items) => assert!(items.is_empty()),
            other => panic!("expected list, got {other:?}"),
        },
        _ => panic!(),
    }
}

#[test]
fn parses_int_list_literal() {
    let q = parse("RETURN [1, 2, 3]").unwrap();
    match &q.clauses[0] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::List(items) => {
                assert_eq!(items.len(), 3);
                assert!(matches!(items[0], Expr::Literal(Literal::Int(1))));
                assert!(matches!(items[2], Expr::Literal(Literal::Int(3))));
            }
            other => panic!("expected list, got {other:?}"),
        },
        _ => panic!(),
    }
}

#[test]
fn parses_mixed_list_literal() {
    let q = parse("RETURN [1, 'two', $three, true, null]").unwrap();
    match &q.clauses[0] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::List(items) => assert_eq!(items.len(), 5),
            _ => panic!(),
        },
        _ => panic!(),
    }
}

#[test]
fn parses_in_operator_with_list() {
    let q = parse("MATCH (u) WHERE u.role IN ['admin', 'owner'] RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary {
            op: BinOp::In,
            lhs,
            rhs,
        }) => {
            assert!(matches!(lhs.as_ref(), Expr::Property { .. }));
            assert!(matches!(rhs.as_ref(), Expr::List(items) if items.len() == 2));
        }
        c => panic!("expected IN, got {c:?}"),
    }
}

#[test]
fn parses_in_operator_with_param() {
    let q = parse("MATCH (u) WHERE u.id IN $allowed RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::In, .. }) => {}
        c => panic!("expected IN, got {c:?}"),
    }
}

#[test]
fn parses_full_kitchen_sink() {
    let q = parse(
        "OPTIONAL MATCH (u:User)-[:FOLLOWS]->(f) \
         WHERE f.role IN ['admin', 'owner'] AND u.active = true \
         RETURN f.name AS name, f.score \
         ORDER BY f.score DESC \
         SKIP 0 LIMIT 10",
    )
    .unwrap();
    assert_eq!(q.clauses.len(), 6);
    assert!(matches!(&q.clauses[0], Clause::Match(m) if m.optional));
    assert!(matches!(&q.clauses[1], Clause::Where(_)));
    assert!(matches!(&q.clauses[2], Clause::Return(_)));
    assert!(matches!(&q.clauses[3], Clause::OrderBy(_)));
    assert!(matches!(&q.clauses[4], Clause::Skip(_)));
    assert!(matches!(&q.clauses[5], Clause::Limit(_)));
}

#[test]
fn rejects_order_by_without_by() {
    assert!(parse("MATCH (u) RETURN u ORDER u.name").is_err());
}

#[test]
fn rejects_unclosed_list() {
    assert!(parse("RETURN [1, 2").is_err());
}
