//! v0.4 logical plan lowering tests.

use cypher_rs::*;

#[test]
fn empty_query_errors() {
    let q = Query { clauses: vec![] };
    assert_eq!(plan(&q), Err(PlanError::EmptyQuery));
}

#[test]
fn return_only_lowers_to_project_over_empty() {
    let q = parse("RETURN 1").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Project { input, exprs } => {
            assert!(matches!(*input, Plan::Empty));
            assert_eq!(exprs.len(), 1);
        }
        other => panic!("expected Project, got {other:?}"),
    }
}

#[test]
fn match_return_lowers_to_project_over_scan() {
    let q = parse("MATCH (u:User) RETURN u").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Scan { var, label } => {
                assert_eq!(var.as_deref(), Some("u"));
                assert_eq!(label.as_deref(), Some("User"));
            }
            other => panic!("expected Scan, got {other:?}"),
        },
        other => panic!("expected Project, got {other:?}"),
    }
}

#[test]
fn match_where_return_inserts_filter() {
    let q = parse("MATCH (u) WHERE u.id = 1 RETURN u").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Project { input, .. } => assert!(matches!(*input, Plan::Filter { .. })),
        other => panic!("expected Project over Filter, got {other:?}"),
    }
}

#[test]
fn match_with_relationship_lowers_to_expand() {
    let q = parse("MATCH (a)-[:KNOWS]->(b) RETURN b").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Expand {
                src,
                rel_types,
                direction,
                dst,
                input: scan,
                ..
            } => {
                assert_eq!(src.as_deref(), Some("a"));
                assert_eq!(rel_types, vec!["KNOWS".to_string()]);
                assert_eq!(direction, Direction::Outgoing);
                assert_eq!(dst.as_deref(), Some("b"));
                assert!(matches!(*scan, Plan::Scan { .. }));
            }
            other => panic!("expected Expand, got {other:?}"),
        },
        other => panic!("expected Project, got {other:?}"),
    }
}

#[test]
fn multi_hop_relationship_chain_threads_src_correctly() {
    let q = parse("MATCH (a)-[:R]->(b)-[:S]->(c) RETURN c").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Expand {
                src,
                rel_types,
                input: inner,
                ..
            } => {
                // outer expand: b -[:S]-> c
                assert_eq!(src.as_deref(), Some("b"));
                assert_eq!(rel_types, vec!["S".to_string()]);
                match *inner {
                    Plan::Expand {
                        src: src2,
                        rel_types: rt2,
                        ..
                    } => {
                        // inner expand: a -[:R]-> b
                        assert_eq!(src2.as_deref(), Some("a"));
                        assert_eq!(rt2, vec!["R".to_string()]);
                    }
                    other => panic!("expected inner Expand, got {other:?}"),
                }
            }
            other => panic!("expected outer Expand, got {other:?}"),
        },
        other => panic!("expected Project, got {other:?}"),
    }
}

#[test]
fn order_by_stacks_above_project() {
    let q = parse("MATCH (u) RETURN u.name ORDER BY u.name").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Sort { input, keys } => {
            assert_eq!(keys.len(), 1);
            assert!(matches!(*input, Plan::Project { .. }));
        }
        other => panic!("expected Sort, got {other:?}"),
    }
}

#[test]
fn limit_above_skip_above_sort() {
    let q = parse("MATCH (u) RETURN u ORDER BY u.created DESC SKIP 5 LIMIT 10").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Limit { input: a, .. } => match *a {
            Plan::Skip { input: b, .. } => match *b {
                Plan::Sort { input: c, .. } => assert!(matches!(*c, Plan::Project { .. })),
                other => panic!("expected Sort, got {other:?}"),
            },
            other => panic!("expected Skip, got {other:?}"),
        },
        other => panic!("expected Limit, got {other:?}"),
    }
}

#[test]
fn alias_preserved_in_project() {
    let q = parse("MATCH (u) RETURN u.name AS n").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Project { exprs, .. } => assert_eq!(exprs[0].alias.as_deref(), Some("n")),
        other => panic!("expected Project, got {other:?}"),
    }
}

#[test]
fn optional_match_without_anchor_errors() {
    let q = parse("OPTIONAL MATCH (u) RETURN u").unwrap();
    assert_eq!(plan(&q), Err(PlanError::OptionalMatchWithoutAnchor));
}

#[test]
fn multi_pattern_lowers_to_cartesian() {
    let q = parse("MATCH (u), (v) RETURN u").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Cartesian { left, right } => {
                assert!(matches!(*left, Plan::Scan { .. }));
                assert!(matches!(*right, Plan::Scan { .. }));
            }
            other => panic!("expected Cartesian, got {other:?}"),
        },
        other => panic!("expected Project, got {other:?}"),
    }
}

#[test]
fn multiple_match_lowers_to_cartesian() {
    let q = parse("MATCH (u) MATCH (v) RETURN u").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Project { input, .. } => assert!(matches!(*input, Plan::Cartesian { .. })),
        other => panic!("expected Project over Cartesian, got {other:?}"),
    }
}

#[test]
fn optional_match_after_match_lowers_to_optional() {
    let q = parse("MATCH (u:User) OPTIONAL MATCH (u)-[:FOLLOWS]->(f) RETURN u, f").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Optional {
                input: i,
                optional: o,
            } => {
                assert!(matches!(*i, Plan::Scan { .. }));
                // Optional branch is itself an Expand-over-Scan
                assert!(matches!(*o, Plan::Expand { .. }));
            }
            other => panic!("expected Optional, got {other:?}"),
        },
        other => panic!("expected Project, got {other:?}"),
    }
}

#[test]
fn three_pattern_match_chains_left_deep() {
    let q = parse("MATCH (a), (b), (c) RETURN a").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Project { input, .. } => match *input {
            // Left-deep: Cartesian(Cartesian(a, b), c)
            Plan::Cartesian { left, right } => {
                assert!(matches!(*left, Plan::Cartesian { .. }));
                assert!(matches!(*right, Plan::Scan { .. }));
            }
            other => panic!("expected Cartesian, got {other:?}"),
        },
        _ => panic!(),
    }
}

#[test]
fn pretty_print_includes_tree_structure() {
    let q = parse("MATCH (u:User) RETURN u.name").unwrap();
    let p = plan(&q).unwrap();
    let s = format!("{p}");
    assert!(s.contains("Project"));
    assert!(s.contains("Scan"));
    // Indented child indicator
    assert!(s.contains("└──"));
}

#[test]
fn full_query_structure() {
    let q = parse(
        "MATCH (u:User)-[:FOLLOWS]->(f:User) \
         WHERE u.id = $uid \
         RETURN f.name AS name \
         ORDER BY f.score DESC \
         LIMIT 10",
    )
    .unwrap();
    let p = plan(&q).unwrap();
    // Walk: Limit > Sort > Project > Filter > Expand > Scan
    let mut node = &p;
    assert!(matches!(node, Plan::Limit { .. }));
    if let Plan::Limit { input, .. } = node {
        node = input;
    }
    assert!(matches!(node, Plan::Sort { .. }));
    if let Plan::Sort { input, .. } = node {
        node = input;
    }
    assert!(matches!(node, Plan::Project { .. }));
    if let Plan::Project { input, .. } = node {
        node = input;
    }
    assert!(matches!(node, Plan::Filter { .. }));
    if let Plan::Filter { input, .. } = node {
        node = input;
    }
    assert!(matches!(node, Plan::Expand { .. }));
    if let Plan::Expand { input, .. } = node {
        node = input;
    }
    assert!(matches!(node, Plan::Scan { .. }));
}
