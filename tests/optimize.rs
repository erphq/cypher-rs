//! v0.6 optimizer tests.

use cypher_rs::*;

fn parse_plan_optimize(src: &str) -> Plan {
    let q = parse(src).unwrap();
    let p = plan(&q).unwrap();
    optimize(p)
}

#[test]
fn pushes_filter_through_project() {
    let p = parse_plan_optimize("MATCH (u) WHERE u.id = 1 RETURN u.name");
    // Expected: Project(Filter(Scan, pred))
    match p {
        Plan::Project { input, .. } => {
            assert!(
                matches!(*input, Plan::Filter { .. }),
                "expected Project over Filter after pushdown, got {input:?}"
            );
        }
        other => panic!("expected Project root, got {other:?}"),
    }
}

#[test]
fn pushes_filter_through_sort() {
    let p = parse_plan_optimize("MATCH (u) WHERE u.active = true RETURN u ORDER BY u.created DESC");
    // Sort > Project > Filter > Scan after pushdown
    match p {
        Plan::Sort { input, .. } => match *input {
            Plan::Project { input: i2, .. } => {
                assert!(matches!(*i2, Plan::Filter { .. }));
            }
            other => panic!("expected Project, got {other:?}"),
        },
        other => panic!("expected Sort root, got {other:?}"),
    }
}

#[test]
fn does_not_push_through_limit() {
    let p = parse_plan_optimize("MATCH (u) RETURN u LIMIT 10");
    // No filter at all in this query - sanity test that pushdown
    // doesn't synthesize one.
    match p {
        Plan::Limit { input, .. } => match *input {
            Plan::Project { input: i2, .. } => {
                assert!(matches!(*i2, Plan::Scan { .. }));
            }
            other => panic!("expected Project under Limit, got {other:?}"),
        },
        other => panic!("expected Limit root, got {other:?}"),
    }
}

#[test]
fn pushes_filter_into_left_side_of_cartesian() {
    let p = parse_plan_optimize("MATCH (u), (v) WHERE u.id = 1 RETURN u, v");
    // Project > Cartesian(Filter(Scan u), Scan v)
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Cartesian { left, right } => {
                assert!(
                    matches!(*left, Plan::Filter { .. }),
                    "expected Filter on left, got {left:?}"
                );
                assert!(matches!(*right, Plan::Scan { .. }));
            }
            other => panic!("expected Cartesian, got {other:?}"),
        },
        _ => panic!(),
    }
}

#[test]
fn pushes_filter_into_right_side_of_cartesian() {
    let p = parse_plan_optimize("MATCH (u), (v) WHERE v.id = 1 RETURN u, v");
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Cartesian { left, right } => {
                assert!(matches!(*left, Plan::Scan { .. }));
                assert!(
                    matches!(*right, Plan::Filter { .. }),
                    "expected Filter on right, got {right:?}"
                );
            }
            other => panic!("expected Cartesian, got {other:?}"),
        },
        _ => panic!(),
    }
}

#[test]
fn keeps_filter_above_cartesian_when_predicate_spans_both() {
    let p = parse_plan_optimize("MATCH (u), (v) WHERE u.id = v.id RETURN u, v");
    // Predicate uses both sides - must stay above the Cartesian.
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Filter { input: i2, .. } => {
                assert!(matches!(*i2, Plan::Cartesian { .. }));
            }
            other => panic!("expected Filter over Cartesian, got {other:?}"),
        },
        _ => panic!(),
    }
}

#[test]
fn does_not_push_filter_referencing_project_alias() {
    let p = parse_plan_optimize("MATCH (u) RETURN u.score AS s ORDER BY s LIMIT 5");
    // The query has no WHERE - check that ORDER BY on alias `s`
    // doesn't accidentally trigger pushdown of nothing.
    let s = format!("{p}");
    assert!(s.contains("Project"));
    assert!(s.contains("Sort"));
}

#[test]
fn idempotent_after_one_pass() {
    let q = parse("MATCH (u:User) WHERE u.id = $uid RETURN u.name").unwrap();
    let p = plan(&q).unwrap();
    let once = optimize(p.clone());
    let twice = optimize(once.clone());
    assert_eq!(once, twice);
}

#[test]
fn preserves_no_op_plans() {
    let q = parse("RETURN 1").unwrap();
    let p = plan(&q).unwrap();
    let opt = optimize(p.clone());
    assert_eq!(p, opt);
}

#[test]
fn does_not_push_through_optional() {
    let p = parse_plan_optimize(
        "MATCH (u:User) OPTIONAL MATCH (u)-[:FOLLOWS]->(f) WHERE f.role = 'admin' RETURN u, f",
    );
    // The WHERE references `f` which is bound by the OPTIONAL branch.
    // The filter must NOT push into the optional branch (that would
    // change semantics - null rows would survive vs. be filtered).
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Filter { input: i2, .. } => {
                assert!(matches!(*i2, Plan::Optional { .. }));
            }
            other => panic!("expected Filter over Optional, got {other:?}"),
        },
        _ => panic!(),
    }
}

#[test]
fn cartesian_with_filter_and_unrelated_other_side() {
    // (u, v) WHERE u.x = 1: filter pushes to u side, v side untouched.
    let p = parse_plan_optimize("MATCH (u:User), (v:Post) WHERE u.x = 1 RETURN u, v");
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Cartesian { left, right } => {
                // Left is Filter(Scan u:User), right is Scan v:Post
                if let Plan::Filter { input: li, .. } = *left {
                    if let Plan::Scan { label, .. } = *li {
                        assert_eq!(label.as_deref(), Some("User"));
                    } else {
                        panic!("expected Scan(User) under Filter on left");
                    }
                } else {
                    panic!("expected Filter on left");
                }
                if let Plan::Scan { label, .. } = *right {
                    assert_eq!(label.as_deref(), Some("Post"));
                } else {
                    panic!("expected Scan(Post) on right");
                }
            }
            other => panic!("expected Cartesian, got {other:?}"),
        },
        _ => panic!(),
    }
}
