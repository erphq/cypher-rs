//! Tests for pattern-shape features that had no dedicated test file:
//! relationship direction variants (incoming, undirected), multiple labels,
//! inline property maps on nodes and relationships, multi-type rels with `|`,
//! and anonymous (unbound) pattern elements.
//!
//! All constructs are parsed by the existing grammar; this file adds coverage
//! that was missing from parse.rs, plan.rs, and sema.rs.

use cypher_rs::*;

// ---- relationship direction ---------------------------------------------

#[test]
fn parses_incoming_direction() {
    let q = parse("MATCH (a)<-[:KNOWS]-(b) RETURN a").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            assert_eq!(m.patterns[0].chain[0].rel.direction, Direction::Incoming);
        }
        other => panic!("expected MATCH, got {other:?}"),
    }
}

#[test]
fn parses_undirected_direction() {
    let q = parse("MATCH (a)-[:KNOWS]-(b) RETURN a").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            assert_eq!(m.patterns[0].chain[0].rel.direction, Direction::Undirected);
        }
        other => panic!("expected MATCH, got {other:?}"),
    }
}

#[test]
fn direction_roundtrip_in_plan_display() {
    // Incoming and undirected must survive plan lowering and be reflected in
    // the plan display string so they remain distinguishable.
    for (src, marker) in [
        ("MATCH (a)-[:K]->(b) RETURN a", "->"),
        ("MATCH (a)<-[:K]-(b) RETURN a", "<-"),
        ("MATCH (a)-[:K]-(b) RETURN a", "--"),
    ] {
        let q = parse(src).unwrap();
        let p = plan(&q).unwrap();
        let s = p.to_string();
        assert!(
            s.contains(marker),
            "plan for `{src}` should contain `{marker}`:\n{s}"
        );
    }
}

// ---- multiple labels on a node ------------------------------------------

#[test]
fn parses_multiple_labels() {
    let q = parse("MATCH (u:Admin:User) RETURN u").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            let labels = &m.patterns[0].anchor.labels;
            assert!(
                labels.contains(&"Admin".to_string()),
                "missing Admin in {labels:?}"
            );
            assert!(
                labels.contains(&"User".to_string()),
                "missing User in {labels:?}"
            );
        }
        other => panic!("expected MATCH, got {other:?}"),
    }
}

// ---- inline node property maps ------------------------------------------

#[test]
fn parses_inline_node_prop_single() {
    let q = parse("MATCH (u:User {name: 'Alice'}) RETURN u").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            let props = &m.patterns[0].anchor.properties;
            assert_eq!(props.len(), 1, "expected 1 property, got {props:?}");
            assert_eq!(props[0].0, "name");
            assert!(
                matches!(&props[0].1, Expr::Literal(Literal::String(s)) if s == "Alice"),
                "expected String('Alice'), got {:?}",
                props[0].1
            );
        }
        other => panic!("expected MATCH, got {other:?}"),
    }
}

#[test]
fn parses_inline_node_prop_multiple() {
    let q = parse("MATCH (u {name: 'Alice', age: 30}) RETURN u").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            let props = &m.patterns[0].anchor.properties;
            assert_eq!(props.len(), 2, "expected 2 properties, got {props:?}");
            let keys: Vec<&str> = props.iter().map(|(k, _)| k.as_str()).collect();
            assert!(keys.contains(&"name"), "missing 'name' in {keys:?}");
            assert!(keys.contains(&"age"), "missing 'age' in {keys:?}");
        }
        other => panic!("expected MATCH, got {other:?}"),
    }
}

#[test]
fn parses_inline_node_prop_with_param() {
    let q = parse("MATCH (u {id: $uid}) RETURN u").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            let props = &m.patterns[0].anchor.properties;
            assert_eq!(props.len(), 1);
            assert_eq!(props[0].0, "id");
            assert!(
                matches!(&props[0].1, Expr::Param(p) if p == "uid"),
                "expected Param(uid), got {:?}",
                props[0].1
            );
        }
        other => panic!("expected MATCH, got {other:?}"),
    }
}

// ---- inline relationship property maps ----------------------------------

#[test]
fn parses_inline_rel_prop() {
    let q = parse("MATCH (a)-[:KNOWS {since: 2020}]->(b) RETURN a").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            let props = &m.patterns[0].chain[0].rel.properties;
            assert_eq!(props.len(), 1, "expected 1 rel property, got {props:?}");
            assert_eq!(props[0].0, "since");
            assert!(
                matches!(&props[0].1, Expr::Literal(Literal::Int(2020))),
                "expected Int(2020), got {:?}",
                props[0].1
            );
        }
        other => panic!("expected MATCH, got {other:?}"),
    }
}

// ---- inline property lowering to Filter ---------------------------------

#[test]
fn inline_node_prop_plan_is_filter_over_scan() {
    // MATCH (u:User {name: 'Alice'}) -> Plan: Project(Filter(Scan))
    let q = parse("MATCH (u:User {name: 'Alice'}) RETURN u").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Filter { input: fi, .. } => {
                assert!(
                    matches!(*fi, Plan::Scan { .. }),
                    "expected Scan under Filter, got {fi:?}"
                );
            }
            other => panic!("expected Filter under Project, got {other:?}"),
        },
        other => panic!("expected Project at root, got {other:?}"),
    }
}

#[test]
fn inline_rel_prop_plan_is_filter_over_expand() {
    // MATCH (a)-[:KNOWS {since: 2020}]->(b) -> Plan: Project(Filter(Expand(Scan)))
    let q = parse("MATCH (a)-[:KNOWS {since: 2020}]->(b) RETURN a").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Filter { input: fi, .. } => {
                assert!(
                    matches!(*fi, Plan::Expand { .. }),
                    "expected Expand under rel-prop Filter, got {fi:?}"
                );
            }
            other => panic!("expected Filter under Project, got {other:?}"),
        },
        other => panic!("expected Project at root, got {other:?}"),
    }
}

// ---- multiple relationship types (pipe syntax) --------------------------

#[test]
fn parses_multiple_rel_types() {
    let q = parse("MATCH (a)-[:KNOWS|FOLLOWS]->(b) RETURN a").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            let types = &m.patterns[0].chain[0].rel.types;
            assert!(
                types.contains(&"KNOWS".to_string()),
                "missing KNOWS in {types:?}"
            );
            assert!(
                types.contains(&"FOLLOWS".to_string()),
                "missing FOLLOWS in {types:?}"
            );
        }
        other => panic!("expected MATCH, got {other:?}"),
    }
}

// ---- anonymous pattern elements -----------------------------------------

#[test]
fn anonymous_anchor_node_has_no_var() {
    let q = parse("MATCH ()-[:KNOWS]->(u:User) RETURN u").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            assert!(
                m.patterns[0].anchor.var.is_none(),
                "expected anonymous anchor, got {:?}",
                m.patterns[0].anchor.var
            );
        }
        other => panic!("expected MATCH, got {other:?}"),
    }
}

#[test]
fn anonymous_rel_has_no_var() {
    let q = parse("MATCH (a)-[]->(b) RETURN a").unwrap();
    match &q.clauses[0] {
        Clause::Match(m) => {
            assert!(
                m.patterns[0].chain[0].rel.var.is_none(),
                "expected anonymous rel var, got {:?}",
                m.patterns[0].chain[0].rel.var
            );
        }
        other => panic!("expected MATCH, got {other:?}"),
    }
}
