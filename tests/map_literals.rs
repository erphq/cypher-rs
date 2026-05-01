//! v0.8 map-literal tests:
//!   1. Map literals as standalone expressions: `RETURN {a: 1, b: 2}`.
//!   2. Property maps on node patterns: `MATCH (u:User {id: $uid})`.
//!   3. Property maps on rel patterns: `MATCH (u)-[:KNOWS {since: 2020}]->(v)`.
//!   4. Plan lowering: property maps desugar into `Filter` operators.

use cypher_rs::*;

fn pq(src: &str) -> Plan {
    let q = parse(src).unwrap();
    plan(&q).unwrap()
}

#[test]
fn parses_empty_map_expression() {
    let q = parse("RETURN {}").unwrap();
    match &q.clauses[0] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::Map(entries) => assert!(entries.is_empty()),
            other => panic!("expected Map, got {other:?}"),
        },
        _ => panic!(),
    }
}

#[test]
fn parses_simple_map_expression() {
    let q = parse("RETURN {a: 1, b: 2}").unwrap();
    match &q.clauses[0] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::Map(entries) => {
                assert_eq!(entries.len(), 2);
                assert_eq!(entries[0].0, "a");
                assert!(matches!(entries[0].1, Expr::Literal(Literal::Int(1))));
                assert_eq!(entries[1].0, "b");
                assert!(matches!(entries[1].1, Expr::Literal(Literal::Int(2))));
            }
            other => panic!("expected Map, got {other:?}"),
        },
        _ => panic!(),
    }
}

#[test]
fn parses_nested_map_with_mixed_value_types() {
    let q = parse("RETURN {name: 'sd', age: 45, tags: ['a', 'b'], src: $origin}").unwrap();
    match &q.clauses[0] {
        Clause::Return(r) => match &r.items[0].expr {
            Expr::Map(entries) => {
                assert_eq!(entries.len(), 4);
                assert!(matches!(&entries[0].1, Expr::Literal(Literal::String(s)) if s == "sd"));
                assert!(matches!(entries[1].1, Expr::Literal(Literal::Int(45))));
                assert!(matches!(&entries[2].1, Expr::List(items) if items.len() == 2));
                assert!(matches!(&entries[3].1, Expr::Param(p) if p == "origin"));
            }
            _ => panic!(),
        },
        _ => panic!(),
    }
}

#[test]
fn parses_node_pattern_with_property_map() {
    let q = parse("MATCH (u:User {id: $uid, role: 'admin'}) RETURN u").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            let n = &m.patterns[0].anchor;
            assert_eq!(n.var.as_deref(), Some("u"));
            assert_eq!(n.labels, vec!["User".to_string()]);
            assert_eq!(n.properties.len(), 2);
            assert_eq!(n.properties[0].0, "id");
            assert_eq!(n.properties[1].0, "role");
        }
        _ => panic!(),
    }
}

#[test]
fn parses_rel_pattern_with_property_map() {
    let q = parse("MATCH (u)-[:KNOWS {since: 2020}]->(v) RETURN u").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            let chain = &m.patterns[0].chain[0];
            assert_eq!(chain.rel.types, vec!["KNOWS".to_string()]);
            assert_eq!(chain.rel.properties.len(), 1);
            assert_eq!(chain.rel.properties[0].0, "since");
            assert!(matches!(
                chain.rel.properties[0].1,
                Expr::Literal(Literal::Int(2020))
            ));
        }
        _ => panic!(),
    }
}

#[test]
fn property_map_lowers_to_filter() {
    let p = pq("MATCH (u:User {id: 1}) RETURN u");
    // Project > Filter > Scan after lowering (no optimizer).
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Filter { input: scan, pred } => {
                assert!(matches!(*scan, Plan::Scan { .. }));
                // pred is u.id = 1
                match pred {
                    Expr::Binary {
                        op: BinOp::Eq,
                        lhs,
                        rhs,
                    } => {
                        match *lhs {
                            Expr::Property { base, key } => {
                                assert_eq!(key, "id");
                                assert!(matches!(*base, Expr::Variable(v) if v == "u"));
                            }
                            other => panic!("expected Property, got {other:?}"),
                        }
                        assert!(matches!(*rhs, Expr::Literal(Literal::Int(1))));
                    }
                    other => panic!("expected Eq, got {other:?}"),
                }
            }
            other => panic!("expected Filter over Scan, got {other:?}"),
        },
        _ => panic!(),
    }
}

#[test]
fn multi_property_map_lowers_to_and_chain() {
    let p = pq("MATCH (u:User {id: 1, role: 'admin'}) RETURN u");
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Filter { pred, .. } => {
                // pred should be (u.id = 1) AND (u.role = 'admin')
                match pred {
                    Expr::Binary { op: BinOp::And, .. } => {}
                    other => panic!("expected AND, got {other:?}"),
                }
            }
            _ => panic!(),
        },
        _ => panic!(),
    }
}

#[test]
fn rel_property_map_with_bound_var_lowers_to_filter_after_expand() {
    // The rel must have a bound var to attach the filter to. Without
    // one, `[:KNOWS {since: 2020}]` is parsed and stored on the AST
    // but the planner currently can't synthesize an anonymous binding
    // (deferred to v0.9).
    let p = pq("MATCH (u)-[r:KNOWS {since: 2020}]->(v) RETURN v");
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Filter { input: i2, pred } => {
                assert!(matches!(*i2, Plan::Expand { .. }));
                // pred is r.since = 2020
                if let Expr::Binary { lhs, .. } = pred {
                    if let Expr::Property { base, key } = *lhs {
                        assert_eq!(key, "since");
                        assert!(matches!(*base, Expr::Variable(v) if v == "r"));
                    }
                }
            }
            other => panic!("expected Filter over Expand, got {other:?}"),
        },
        _ => panic!(),
    }
}

#[test]
fn rel_property_map_without_bound_var_skips_filter_v0_8() {
    // v0.8 limitation: `[:KNOWS {since: 2020}]` (no var) parses but
    // produces no filter because there's nothing to attach the
    // predicate to. v0.9 will synthesize an internal binding.
    let p = pq("MATCH (u)-[:KNOWS {since: 2020}]->(v) RETURN v");
    match p {
        Plan::Project { input, .. } => assert!(matches!(*input, Plan::Expand { .. })),
        _ => panic!(),
    }
}

#[test]
fn property_map_filter_pushes_through_optimizer() {
    let q = parse("MATCH (u:User {id: 1}) RETURN u.name").unwrap();
    let opt = optimize(plan(&q).unwrap());
    // The filter from the property map should still sit above the
    // scan after optimizing. The optimizer is a fixpoint, so multiple
    // passes don't shuffle the simple Project > Filter > Scan shape.
    match opt {
        Plan::Project { input, .. } => match *input {
            Plan::Filter { input: scan, .. } => {
                assert!(matches!(*scan, Plan::Scan { .. }));
            }
            other => panic!("expected Filter over Scan, got {other:?}"),
        },
        _ => panic!(),
    }
}

#[test]
fn empty_property_map_does_not_introduce_filter() {
    // `(u {})` is legal; it just contributes no constraints.
    let p = pq("MATCH (u {}) RETURN u");
    match p {
        Plan::Project { input, .. } => assert!(matches!(*input, Plan::Scan { .. })),
        _ => panic!(),
    }
}

#[test]
fn map_literal_inner_variables_are_checked_by_sema() {
    let q = parse("MATCH (u) RETURN {self: u, other: x}").unwrap();
    let report = analyze(&q);
    assert!(
        report.has_errors(),
        "expected unbound-variable error for `x`, got {:?}",
        report.issues
    );
}

#[test]
fn property_map_value_can_reference_param() {
    let q = parse("MATCH (u:User {id: $uid}) RETURN u").unwrap();
    let report = analyze(&q);
    assert!(
        !report.has_errors(),
        "expected clean, got {:?}",
        report.issues
    );
}
