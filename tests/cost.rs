//! v0.7 cost-model tests.

use cypher_rs::*;

fn pq(src: &str) -> Plan {
    let q = parse(src).unwrap();
    plan(&q).unwrap()
}

#[test]
fn scan_cost_equals_cardinality() {
    let p = pq("MATCH (u) RETURN u");
    let m = CardinalityCostModel::default();
    let est = estimate(&p, &m);
    // Project layer adds one pass over the input; Scan itself is the input.
    // Cost is at least the scan card.
    assert!(est.cost >= 10_000.0);
}

#[test]
fn label_override_changes_cardinality() {
    let p = pq("MATCH (u:User) RETURN u");
    let m = CardinalityCostModel::default().with_label("User", 50.0);
    let est = estimate(&p, &m);
    // 50 rows from Scan, 50 through Project.
    assert!(est.cardinality < 100.0, "expected ~50 rows, got {est:?}");
}

#[test]
fn filter_reduces_cardinality_by_selectivity() {
    let p = pq("MATCH (u:User) WHERE u.x = 1 RETURN u");
    let m = CardinalityCostModel::default().with_label("User", 1_000.0);
    let est = estimate(&p, &m);
    // selectivity default 0.1 → ~100 rows out of 1000 after Filter.
    assert!(
        est.cardinality < 200.0,
        "expected ~100 rows after filter, got {est:?}"
    );
}

#[test]
fn cartesian_multiplies_cardinality() {
    let p = pq("MATCH (u:User), (p:Post) RETURN u, p");
    let m = CardinalityCostModel::default()
        .with_label("User", 100.0)
        .with_label("Post", 200.0);
    let est = estimate(&p, &m);
    // Cartesian → 100 * 200 = 20 000 rows.
    assert_eq!(est.cardinality, 20_000.0);
}

#[test]
fn expand_uses_rel_fanout() {
    let p = pq("MATCH (u:User)-[:KNOWS]->(f) RETURN f");
    let m = CardinalityCostModel::default()
        .with_label("User", 1_000.0)
        .with_rel("KNOWS", 50.0);
    let est = estimate(&p, &m);
    // 1000 users * 50 fanout = 50 000 rows after Expand.
    assert!(
        est.cardinality >= 49_000.0 && est.cardinality <= 51_000.0,
        "expected ~50000 rows after expand, got {est:?}"
    );
}

#[test]
fn limit_caps_output_cardinality() {
    let p = pq("MATCH (u:User) RETURN u LIMIT 10");
    let m = CardinalityCostModel::default().with_label("User", 100_000.0);
    let est = estimate(&p, &m);
    assert_eq!(est.cardinality, 10.0);
}

#[test]
fn pushdown_makes_plan_cheaper() {
    let q = parse("MATCH (u:User), (v:Post) WHERE u.id = 1 RETURN u, v").unwrap();
    let raw = plan(&q).unwrap();
    let opt = optimize(raw.clone());
    let m = CardinalityCostModel::default()
        .with_label("User", 1_000.0)
        .with_label("Post", 5_000.0);
    let raw_cost = estimate_cost(&raw, &m);
    let opt_cost = estimate_cost(&opt, &m);
    assert!(
        opt_cost < raw_cost,
        "expected pushdown to lower cost: raw={raw_cost} opt={opt_cost}"
    );
}

#[test]
fn sort_adds_n_log_n_term() {
    // Compare a query with and without ORDER BY.
    let no_sort = pq("MATCH (u:User) RETURN u");
    let with_sort = pq("MATCH (u:User) RETURN u ORDER BY u.name");
    let m = CardinalityCostModel::default().with_label("User", 1_000.0);
    let c1 = estimate_cost(&no_sort, &m);
    let c2 = estimate_cost(&with_sort, &m);
    // 1000 * ln(1000) ≈ 6900 of additional cost.
    assert!(
        c2 - c1 > 5_000.0,
        "expected n*ln(n) bump, got delta {}",
        c2 - c1
    );
}

#[test]
fn empty_query_has_zero_cost() {
    let q = Query { clauses: vec![] };
    // plan() would error; build an Empty plan directly.
    let p = Plan::Empty;
    assert_eq!(estimate_cost(&p, &CardinalityCostModel::default()), 0.0);
    let _ = q;
}

#[test]
fn custom_cost_model_via_trait() {
    struct AlwaysOne;
    impl CostModel for AlwaysOne {
        fn scan_cardinality(&self, _label: Option<&str>) -> f64 {
            1.0
        }
        fn expand_fanout(&self, _rel_types: &[String], _direction: Direction) -> f64 {
            1.0
        }
        fn filter_selectivity(&self, _pred: &Expr) -> f64 {
            1.0
        }
    }
    let p = pq("MATCH (u:User)-[:KNOWS]->(v) WHERE u.x = 1 RETURN v");
    let cost = estimate_cost(&p, &AlwaysOne);
    // Tiny graph → cost should be a small handful of rows.
    assert!(cost < 50.0, "expected tiny cost, got {cost}");
}
