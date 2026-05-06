use cypher_rs::*;

#[test]
fn parses_simple_match_return() {
    let q = parse("MATCH (u:User) RETURN u").unwrap();
    assert_eq!(q.clauses.len(), 2);
    match &q.clauses[0] {
        Clause::Match(m) => {
            assert_eq!(m.patterns.len(), 1);
            assert_eq!(m.patterns[0].anchor.var.as_deref(), Some("u"));
            assert_eq!(m.patterns[0].anchor.labels, vec!["User".to_string()]);
        }
        _ => panic!("expected MATCH"),
    }
}

#[test]
fn parses_relationship_chain() {
    let q = parse("MATCH (u:User)-[:FOLLOWS]->(f:User) RETURN f.name").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            let p = &m.patterns[0];
            assert_eq!(p.chain.len(), 1);
            assert_eq!(p.chain[0].rel.direction, Direction::Outgoing);
            assert_eq!(p.chain[0].rel.types, vec!["FOLLOWS".to_string()]);
            assert_eq!(p.chain[0].node.var.as_deref(), Some("f"));
        }
        _ => panic!("expected MATCH"),
    }
}

#[test]
fn parses_where_property_param() {
    let q = parse("MATCH (u:User) WHERE u.id = $uid RETURN u").unwrap();
    assert_eq!(q.clauses.len(), 3);
    match &q.clauses[1] {
        Clause::Where(w) => match w {
            Expr::Binary { op, lhs, rhs } => {
                assert_eq!(*op, BinOp::Eq);
                match lhs.as_ref() {
                    Expr::Property { key, .. } => assert_eq!(key, "id"),
                    other => panic!("expected property, got {other:?}"),
                }
                match rhs.as_ref() {
                    Expr::Param(name) => assert_eq!(name, "uid"),
                    other => panic!("expected param, got {other:?}"),
                }
            }
            other => panic!("expected binary, got {other:?}"),
        },
        _ => panic!("expected WHERE"),
    }
}

#[test]
fn parses_limit_and_skip() {
    let q = parse("MATCH (u) RETURN u LIMIT 10 SKIP 5").unwrap();
    assert_eq!(q.clauses.len(), 4);
    match &q.clauses[2] {
        Clause::Limit(Expr::Literal(Literal::Int(10))) => {}
        other => panic!("expected LIMIT 10, got {other:?}"),
    }
    match &q.clauses[3] {
        Clause::Skip(Expr::Literal(Literal::Int(5))) => {}
        other => panic!("expected SKIP 5, got {other:?}"),
    }
}

#[test]
fn parses_arithmetic_and_compare() {
    let q = parse("MATCH (u) WHERE u.age + 1 > 18 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::Gt, .. }) => {}
        other => panic!("expected top-level Gt, got {other:?}"),
    }
}

#[test]
fn parses_alias() {
    let q = parse("MATCH (u) RETURN u.name AS n").unwrap();
    match &q.clauses[1] {
        Clause::Return(r) => assert_eq!(r.items[0].alias.as_deref(), Some("n")),
        _ => panic!("expected RETURN"),
    }
}

#[test]
fn parses_string_and_float() {
    let q = parse("RETURN 'hello', 12.5").unwrap();
    match &q.clauses[0] {
        Clause::Return(r) => {
            assert_eq!(r.items.len(), 2);
            assert!(matches!(
                &r.items[0].expr,
                Expr::Literal(Literal::String(s)) if s == "hello"
            ));
            assert!(matches!(
                &r.items[1].expr,
                Expr::Literal(Literal::Float(f)) if (*f - 12.5).abs() < 1e-9
            ));
        }
        _ => panic!("expected RETURN"),
    }
}

#[test]
fn parses_boolean_logic() {
    let q = parse("MATCH (u) WHERE u.active = true AND u.age > 18 RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::And, .. }) => {}
        other => panic!("expected AND, got {other:?}"),
    }
}

#[test]
fn parses_not_expression() {
    let q = parse("MATCH (u) WHERE NOT u.banned RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Unary { op: UnOp::Not, .. }) => {}
        other => panic!("expected NOT, got {other:?}"),
    }
}

#[test]
fn parses_undirected_relationship() {
    let q = parse("MATCH (a)-[:KNOWS]-(b) RETURN a, b").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            assert_eq!(m.patterns[0].chain[0].rel.direction, Direction::Undirected);
        }
        _ => panic!(),
    }
}

#[test]
fn parses_incoming_relationship() {
    let q = parse("MATCH (a)<-[:OWNS]-(b) RETURN a").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            assert_eq!(m.patterns[0].chain[0].rel.direction, Direction::Incoming);
        }
        _ => panic!(),
    }
}

#[test]
fn parses_multiple_labels() {
    let q = parse("MATCH (u:User:Active) RETURN u").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            assert_eq!(
                m.patterns[0].anchor.labels,
                vec!["User".to_string(), "Active".to_string()]
            );
        }
        _ => panic!(),
    }
}

#[test]
fn parses_multiple_return_items() {
    let q = parse("MATCH (u) RETURN u.id, u.name AS n, u.age").unwrap();
    match &q.clauses[1] {
        Clause::Return(r) => {
            assert_eq!(r.items.len(), 3);
            assert_eq!(r.items[1].alias.as_deref(), Some("n"));
        }
        _ => panic!(),
    }
}

#[test]
fn rejects_garbage() {
    assert!(parse("HELLO WORLD").is_err());
}

#[test]
fn rejects_empty() {
    assert!(parse("").is_err());
}

#[test]
fn rejects_unknown_keyword() {
    // MATCH must come before RETURN; reversing should still parse since both are clauses,
    // but a typo'd keyword should fail.
    assert!(parse("MATCHX (u) RETURN u").is_err());
}

#[test]
fn parses_paren_expr() {
    let q = parse("MATCH (u) WHERE (u.age + 1) * 2 > 50 RETURN u").unwrap();
    assert_eq!(q.clauses.len(), 3);
}

#[test]
fn parses_nested_property() {
    let q = parse("MATCH (u) RETURN u.profile.name").unwrap();
    match &q.clauses[1] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::Property { base, key } => {
                assert_eq!(key, "name");
                match base.as_ref() {
                    Expr::Property { key: k2, .. } => assert_eq!(k2, "profile"),
                    other => panic!("expected nested property, got {other:?}"),
                }
            }
            other => panic!("expected property, got {other:?}"),
        },
        _ => panic!(),
    }
}

#[test]
fn parses_is_null() {
    let q = parse("MATCH (u:User) WHERE u.email IS NULL RETURN u").unwrap();
    assert_eq!(q.clauses.len(), 3);
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
fn parses_is_not_null() {
    let q = parse("MATCH (u:User) WHERE u.email IS NOT NULL RETURN u").unwrap();
    assert_eq!(q.clauses.len(), 3);
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
fn parses_is_null_combined_with_and() {
    let q = parse("MATCH (u:User) WHERE u.email IS NULL AND u.name IS NOT NULL RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Binary { op: BinOp::And, .. }) => {}
        other => panic!("expected AND at top level, got {other:?}"),
    }
}

#[test]
fn parses_is_null_lowercase() {
    let q = parse("MATCH (u) WHERE u.x is null RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Unary {
            op: UnOp::IsNull, ..
        }) => {}
        other => panic!("expected IS NULL (case-insensitive), got {other:?}"),
    }
}

#[test]
fn parses_is_not_null_lowercase() {
    let q = parse("MATCH (u) WHERE u.x is not null RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::Where(Expr::Unary {
            op: UnOp::IsNotNull,
            ..
        }) => {}
        other => panic!("expected IS NOT NULL (case-insensitive), got {other:?}"),
    }
}
