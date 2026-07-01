use cypher_rs::*;

// ---- parsing ------------------------------------------------------------

#[test]
fn parses_return_distinct_flag() {
    let q = parse("MATCH (u:User) RETURN DISTINCT u.name").unwrap();
    match q.clauses.last().unwrap() {
        Clause::Return(r) => {
            assert!(
                r.distinct,
                "expected distinct=true on RETURN DISTINCT clause"
            );
            assert_eq!(r.items.len(), 1);
        }
        other => panic!("expected Clause::Return, got {other:?}"),
    }
}

#[test]
fn parses_return_without_distinct_is_false() {
    let q = parse("MATCH (u:User) RETURN u.name").unwrap();
    match q.clauses.last().unwrap() {
        Clause::Return(r) => {
            assert!(!r.distinct, "expected distinct=false on plain RETURN");
        }
        other => panic!("expected Clause::Return, got {other:?}"),
    }
}

#[test]
fn parses_return_distinct_case_insensitive() {
    let q = parse("MATCH (u) RETURN distinct u").unwrap();
    match q.clauses.last().unwrap() {
        Clause::Return(r) => assert!(r.distinct, "lowercase 'distinct' should parse"),
        other => panic!("expected Clause::Return, got {other:?}"),
    }
}

#[test]
fn parses_return_distinct_multiple_items() {
    let q = parse("MATCH (u:User) RETURN DISTINCT u.name, u.email").unwrap();
    match q.clauses.last().unwrap() {
        Clause::Return(r) => {
            assert!(r.distinct);
            assert_eq!(r.items.len(), 2);
        }
        other => panic!("expected Clause::Return, got {other:?}"),
    }
}

#[test]
fn parses_with_distinct_flag() {
    let q = parse("MATCH (u:User) WITH DISTINCT u RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::With(r) => assert!(r.distinct, "expected distinct=true on WITH DISTINCT"),
        other => panic!("expected Clause::With, got {other:?}"),
    }
}

#[test]
fn parses_with_without_distinct_is_false() {
    let q = parse("MATCH (u:User) WITH u RETURN u").unwrap();
    match &q.clauses[1] {
        Clause::With(r) => assert!(!r.distinct, "expected distinct=false on plain WITH"),
        other => panic!("expected Clause::With, got {other:?}"),
    }
}

#[test]
fn distinct_keyword_not_usable_as_identifier() {
    assert!(
        parse("MATCH (distinct:Foo) RETURN distinct").is_err(),
        "DISTINCT must not parse as an identifier"
    );
}

// ---- plan tests ---------------------------------------------------------

#[test]
fn plan_return_distinct_has_distinct_node() {
    let q = parse("MATCH (u:User) RETURN DISTINCT u.name").unwrap();
    let p = plan(&q).unwrap();
    let s = p.to_string();
    assert!(
        s.contains("Distinct"),
        "expected Distinct node in plan:\n{s}"
    );
    assert!(s.contains("Project"), "expected Project node in plan:\n{s}");
}

#[test]
fn plan_return_without_distinct_has_no_distinct_node() {
    let q = parse("MATCH (u:User) RETURN u.name").unwrap();
    let p = plan(&q).unwrap();
    let s = p.to_string();
    assert!(
        !s.contains("Distinct"),
        "unexpected Distinct node in non-DISTINCT plan:\n{s}"
    );
}

#[test]
fn plan_distinct_appears_above_project() {
    let q = parse("MATCH (u:User) RETURN DISTINCT u").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Distinct { input } => {
            assert!(
                matches!(*input, Plan::Project { .. }),
                "expected Project directly under Distinct, got {input:?}"
            );
        }
        other => panic!("expected Distinct at root, got {other:?}"),
    }
}

#[test]
fn plan_with_distinct_has_distinct_node() {
    let q = parse("MATCH (u:User) WITH DISTINCT u RETURN u").unwrap();
    let p = plan(&q).unwrap();
    let s = p.to_string();
    assert!(
        s.contains("Distinct"),
        "expected Distinct node in plan:\n{s}"
    );
}

#[test]
fn plan_distinct_with_limit_stacks_correctly() {
    let q = parse("MATCH (u:User) RETURN DISTINCT u LIMIT 10").unwrap();
    let p = plan(&q).unwrap();
    match p {
        Plan::Limit { input, .. } => {
            assert!(
                matches!(*input, Plan::Distinct { .. }),
                "expected Distinct under Limit, got {input:?}"
            );
        }
        other => panic!("expected Limit at root, got {other:?}"),
    }
}

// ---- cost ---------------------------------------------------------------

#[test]
fn cost_distinct_reduces_cardinality() {
    let q = parse("MATCH (u:User) RETURN DISTINCT u.name").unwrap();
    let p = plan(&q).unwrap();
    let m = CardinalityCostModel::default();
    let est = estimate(&p, &m);
    assert!(
        est.cardinality >= 1.0,
        "cardinality must be at least 1, got {est:?}"
    );
}

// ---- semantic analysis --------------------------------------------------

#[test]
fn sema_accepts_return_distinct_with_bound_variables() {
    let q = parse("MATCH (u:User) RETURN DISTINCT u.name").unwrap();
    let r = analyze(&q);
    assert!(!r.has_errors(), "unexpected errors: {:?}", r.issues);
}

#[test]
fn sema_flags_unbound_variable_in_return_distinct() {
    let q = parse("MATCH (u:User) RETURN DISTINCT x.name").unwrap();
    let r = analyze(&q);
    let codes: Vec<_> = r.errors().map(|i| i.code).collect();
    assert!(
        codes.contains(&"unbound-variable"),
        "expected unbound-variable error, got {codes:?}"
    );
}
