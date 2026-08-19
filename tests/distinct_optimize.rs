//! Optimizer tests for filter pushdown through Distinct nodes.
//!
//! `filter(distinct(X), pred) = distinct(filter(X, pred))` for any
//! pure predicate. The optimizer exploits this to push filters below
//! Distinct so fewer rows enter the dedup step.

use cypher_rs::*;

fn opt(src: &str) -> Plan {
    let q = parse(src).unwrap();
    let p = plan(&q).unwrap();
    optimize(p)
}

// ---- filter pushes through Distinct ------------------------------------

#[test]
fn filter_pushed_through_with_distinct_passthrough_var() {
    // WITH DISTINCT u passes u through unchanged. The WHERE predicate
    // references u, which exists below the Distinct, so the filter
    // should be pushed below the Distinct node.
    let p = opt("MATCH (u:User) WITH DISTINCT u WHERE u.active = true RETURN u");
    // Expected after optimization: Project > Distinct > Project > Filter > Scan
    match p {
        Plan::Project { input: outer, .. } => match *outer {
            Plan::Distinct { input: di } => {
                let s = format!("{}", *di);
                assert!(
                    s.contains("Filter"),
                    "expected Filter below Distinct, got:\n{s}"
                );
            }
            other => panic!("expected Distinct under outer Project, got {other:?}"),
        },
        other => panic!("expected Project at root, got {other:?}"),
    }
}

#[test]
fn filter_pushed_through_with_distinct_renamed_alias() {
    // WITH DISTINCT u.name AS name renames the column. The WHERE predicate
    // references `name`, which is the alias from the WITH project.
    // The filter can still move below the Distinct (though not below the
    // WITH project, since `name` doesn't exist below it).
    let p = opt("MATCH (u:User) WITH DISTINCT u.name AS name WHERE name = 'Alice' RETURN name");
    // Expected: Project > Distinct > Filter > Project(WITH) > Scan
    match p {
        Plan::Project { input: outer, .. } => match *outer {
            Plan::Distinct { input: di } => {
                assert!(
                    matches!(*di, Plan::Filter { .. }),
                    "expected Filter directly under Distinct, got {di:?}"
                );
            }
            other => panic!("expected Distinct under outer Project, got {other:?}"),
        },
        other => panic!("expected Project at root, got {other:?}"),
    }
}

#[test]
fn filter_not_above_distinct_after_optimization() {
    // After optimization the filter must not sit above the Distinct node.
    // If it does, that means the pushdown did not fire.
    let p = opt("MATCH (u:User) WITH DISTINCT u WHERE u.id = 1 RETURN u");
    let s = p.to_string();
    // Walk the string: Distinct must appear before (i.e. deeper than) Filter.
    let distinct_pos = s.find("Distinct").expect("expected Distinct in plan");
    let filter_pos = s.find("Filter").expect("expected Filter in plan");
    assert!(
        filter_pos > distinct_pos,
        "Filter should appear below (later in) Distinct in the plan display:\n{s}"
    );
}

// ---- idempotence with Distinct ----------------------------------------

#[test]
fn optimizer_idempotent_with_return_distinct() {
    let q = parse("MATCH (u:User) WHERE u.id = $uid RETURN DISTINCT u.name").unwrap();
    let p = plan(&q).unwrap();
    let once = optimize(p);
    let twice = optimize(once.clone());
    assert_eq!(
        once, twice,
        "optimizer must be idempotent on DISTINCT queries"
    );
}

#[test]
fn optimizer_idempotent_with_with_distinct_and_filter() {
    let q = parse("MATCH (u:User) WITH DISTINCT u WHERE u.active = true RETURN u").unwrap();
    let p = plan(&q).unwrap();
    let once = optimize(p);
    let twice = optimize(once.clone());
    assert_eq!(
        once, twice,
        "optimizer must be idempotent on WITH DISTINCT + WHERE queries"
    );
}

// ---- cost improvement --------------------------------------------------

#[test]
fn filter_through_distinct_lowers_cost() {
    // The filter should move below the Distinct, so the Distinct
    // processes a smaller set. The optimized plan must cost no more
    // than the raw plan.
    let q = parse("MATCH (u:User) WITH DISTINCT u WHERE u.active = true RETURN u").unwrap();
    let raw = plan(&q).unwrap();
    let opt_p = optimize(raw.clone());
    let m = CardinalityCostModel::default().with_label("User", 1_000.0);
    let raw_cost = estimate_cost(&raw, &m);
    let opt_cost = estimate_cost(&opt_p, &m);
    assert!(
        opt_cost <= raw_cost,
        "expected optimized cost <= raw cost: raw={raw_cost} opt={opt_cost}"
    );
}

// ---- descend into RETURN DISTINCT input --------------------------------

#[test]
fn descend_pushes_filter_into_return_distinct_input() {
    // RETURN DISTINCT: the Distinct wraps the Project. A WHERE above
    // the whole thing would be unusual (the parser won't generate it
    // directly), but the optimizer must correctly descend and push any
    // filter that was already below the Distinct further down.
    // Concretely: MATCH (u) WHERE u.id = 1 RETURN DISTINCT u.name
    // already has Filter below Project. Optimizer should push Filter
    // below Project to reach the Scan.
    let p = opt("MATCH (u) WHERE u.id = 1 RETURN DISTINCT u.name");
    // Expected: Distinct > Project > Filter > Scan
    match p {
        Plan::Distinct { input: di } => match *di {
            Plan::Project { input: pi, .. } => {
                assert!(
                    matches!(*pi, Plan::Filter { .. }),
                    "expected Filter below Project inside Distinct, got {pi:?}"
                );
            }
            other => panic!("expected Project under Distinct, got {other:?}"),
        },
        other => panic!("expected Distinct at root, got {other:?}"),
    }
}
