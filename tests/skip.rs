//! Tests for the SKIP clause: parse, plan shape, optimizer barrier, and cost.
//!
//! SKIP was added in v0.1 but has no dedicated test file. The only prior
//! coverage is the kitchen-sink parse test in grammar_v0_2.rs.

use cypher_rs::*;

fn parse_plan(src: &str) -> Plan {
    let q = parse(src).unwrap();
    plan(&q).unwrap()
}

fn parse_plan_opt(src: &str) -> Plan {
    let q = parse(src).unwrap();
    let p = plan(&q).unwrap();
    optimize(p)
}

// --- parsing ------------------------------------------------------------

#[test]
fn parses_skip_integer() {
    let q = parse("MATCH (u:User) RETURN u SKIP 5").unwrap();
    match q.clauses.last().unwrap() {
        Clause::Skip(Expr::Literal(Literal::Int(n))) => assert_eq!(*n, 5),
        other => panic!("expected Skip(Int(5)), got {other:?}"),
    }
}

#[test]
fn parses_skip_param() {
    let q = parse("MATCH (u:User) RETURN u SKIP $offset").unwrap();
    match q.clauses.last().unwrap() {
        Clause::Skip(Expr::Param(p)) => assert_eq!(p, "offset"),
        other => panic!("expected Skip(Param(\"offset\")), got {other:?}"),
    }
}

#[test]
fn parses_skip_zero() {
    // SKIP 0 is a no-op but must be syntactically valid.
    let q = parse("RETURN 1 SKIP 0").unwrap();
    assert!(
        matches!(q.clauses.last().unwrap(), Clause::Skip(_)),
        "expected a Skip clause"
    );
}

#[test]
fn parses_skip_without_limit() {
    // SKIP without a trailing LIMIT is valid openCypher.
    let q = parse("MATCH (u:User) RETURN u SKIP 10").unwrap();
    assert_eq!(q.clauses.len(), 3);
    assert!(matches!(&q.clauses[2], Clause::Skip(_)));
}

#[test]
fn parses_skip_before_limit() {
    let q = parse("MATCH (u:User) RETURN u SKIP 5 LIMIT 10").unwrap();
    assert!(matches!(&q.clauses[2], Clause::Skip(_)));
    assert!(matches!(&q.clauses[3], Clause::Limit(_)));
}

#[test]
fn skip_keyword_case_insensitive() {
    let q = parse("MATCH (u) RETURN u skip 3").unwrap();
    assert!(
        matches!(q.clauses.last().unwrap(), Clause::Skip(_)),
        "lowercase 'skip' must parse"
    );
}

// --- plan shape ---------------------------------------------------------

#[test]
fn skip_produces_skip_node_in_plan() {
    let p = parse_plan("MATCH (u:User) RETURN u SKIP 5");
    let s = p.to_string();
    assert!(s.contains("Skip"), "expected Skip node in plan:\n{s}");
}

#[test]
fn skip_stacks_below_limit() {
    // Plan stacking order: Limit is the root, Skip is its direct input.
    let p = parse_plan("MATCH (u:User) RETURN u SKIP 5 LIMIT 10");
    match p {
        Plan::Limit { input, .. } => {
            assert!(
                matches!(*input, Plan::Skip { .. }),
                "expected Skip directly under Limit, got {input:?}"
            );
        }
        other => panic!("expected Limit at root, got {other:?}"),
    }
}

#[test]
fn skip_stacks_above_sort() {
    // Full post-RETURN stack: Limit > Skip > Sort > Project > ...
    let p = parse_plan("MATCH (u:User) RETURN u ORDER BY u.name SKIP 5 LIMIT 10");
    match p {
        Plan::Limit {
            input: limit_in, ..
        } => match *limit_in {
            Plan::Skip { input: skip_in, .. } => {
                assert!(
                    matches!(*skip_in, Plan::Sort { .. }),
                    "expected Sort under Skip, got {skip_in:?}"
                );
            }
            other => panic!("expected Skip under Limit, got {other:?}"),
        },
        other => panic!("expected Limit at root, got {other:?}"),
    }
}

#[test]
fn skip_with_count_expression() {
    // Verify the Skip node carries the literal count.
    let p = parse_plan("MATCH (u) RETURN u SKIP 20");
    match p {
        Plan::Skip { count, .. } => {
            assert!(
                matches!(count, Expr::Literal(Literal::Int(20))),
                "expected Int(20) as skip count, got {count:?}"
            );
        }
        other => panic!("expected Skip at root, got {other:?}"),
    }
}

// --- optimizer ----------------------------------------------------------

#[test]
fn optimizer_preserves_skip_node() {
    // The optimizer must not elide or reorder the Skip node. This verifies
    // that running the pushdown rewrite to fixpoint leaves the Skip in place.
    let p = parse_plan_opt("MATCH (u) RETURN u SKIP 5");
    match p {
        Plan::Skip { input, .. } => match *input {
            Plan::Project { input: i2, .. } => {
                assert!(
                    matches!(*i2, Plan::Scan { .. }),
                    "expected Scan under Project, got {i2:?}"
                );
            }
            other => panic!("expected Project under Skip, got {other:?}"),
        },
        other => panic!("expected Skip at root, got {other:?}"),
    }
}

#[test]
fn optimizer_does_not_push_filter_through_skip() {
    // A Filter that the optimizer might try to push down must not cross a
    // Skip boundary. The filter is placed before RETURN in the clause
    // sequence, so in the plan it lands below the Project. The optimizer
    // should push it as far down as possible (below Project, above Scan)
    // but must leave the Skip node at the top, untouched.
    let p = parse_plan_opt("MATCH (u:User) WHERE u.id = 1 RETURN u SKIP 5");
    // Expected shape after optimize: Skip > Project > Filter > Scan
    let s = p.to_string();
    assert!(s.contains("Skip"), "Skip must still be present:\n{s}");
    assert!(s.contains("Filter"), "Filter must still be present:\n{s}");
    // Confirm root is Skip, not Filter.
    assert!(
        matches!(p, Plan::Skip { .. }),
        "expected Skip at root after optimize, got {p:?}"
    );
}

// --- cost model ---------------------------------------------------------

#[test]
fn cost_skip_reduces_cardinality() {
    // After SKIP n, the row count is input_cardinality - n.
    // With default CardinalityCostModel: Scan(User) = 10_000 rows,
    // SKIP 100 -> 9_900 rows.
    let q = parse("MATCH (u:User) RETURN u SKIP 100").unwrap();
    let p = plan(&q).unwrap();
    let m = CardinalityCostModel::default().with_label("User", 10_000.0);
    let est = estimate(&p, &m);
    assert!(
        (est.cardinality - 9_900.0).abs() < 1.0,
        "expected cardinality ~9900 after SKIP 100, got {est:?}"
    );
}

#[test]
fn cost_skip_beyond_input_gives_zero_cardinality() {
    // SKIP larger than the input row count must clamp to 0, not go negative.
    let q = parse("MATCH (u:User) RETURN u SKIP 50000").unwrap();
    let p = plan(&q).unwrap();
    let m = CardinalityCostModel::default().with_label("User", 100.0);
    let est = estimate(&p, &m);
    assert!(
        est.cardinality == 0.0,
        "expected cardinality 0 when skip > rows, got {est:?}"
    );
}

#[test]
fn cost_skip_zero_does_not_change_cardinality() {
    // SKIP 0 is a no-op: cardinality should equal the input cardinality
    // (minus the Project overhead, which is pass-through).
    let q = parse("MATCH (u:User) RETURN u SKIP 0").unwrap();
    let p = plan(&q).unwrap();
    let m = CardinalityCostModel::default().with_label("User", 200.0);
    let est = estimate(&p, &m);
    // Project passes cardinality through. Skip 0 also passes through.
    assert!(
        (est.cardinality - 200.0).abs() < 1.0,
        "expected cardinality unchanged by SKIP 0, got {est:?}"
    );
}
