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

// ---- plan shape ---------------------------------------------------------

#[test]
fn with_plus_order_by_produces_sort_node() {
    let p = parse_plan("MATCH (u:User) WITH u.name AS name ORDER BY name RETURN name");
    let s = p.to_string();
    assert!(s.contains("Sort"), "expected Sort node in plan:\n{s}");
    assert!(
        s.contains("Project"),
        "expected at least one Project in plan:\n{s}"
    );
}

#[test]
fn with_plus_limit_produces_limit_node() {
    let p = parse_plan("MATCH (u:User) WITH u LIMIT 5 RETURN u");
    let s = p.to_string();
    assert!(s.contains("Limit"), "expected Limit node in plan:\n{s}");
}

// ---- optimizer interactions ---------------------------------------------

#[test]
fn optimizer_filter_on_with_alias_stays_above_with_project() {
    // WHERE predicate references an alias introduced by WITH. The optimizer
    // must not push the filter below the WITH project because the alias does
    // not exist in the rows produced below that node.
    let p =
        parse_plan_opt("MATCH (u:User) WITH u.email AS email WHERE email = 'test' RETURN email");
    // Expected shape: Project(RETURN) > Filter > Project(WITH) > Scan
    match p {
        Plan::Project { input, .. } => match *input {
            Plan::Filter { input: fi, .. } => {
                assert!(
                    matches!(*fi, Plan::Project { .. }),
                    "expected WITH project below filter, got {fi:?}"
                );
            }
            other => panic!("expected Filter above WITH project, got {other:?}"),
        },
        other => panic!("expected outer Project root, got {other:?}"),
    }
}

#[test]
fn optimizer_filter_on_passthrough_var_pushes_below_with_project() {
    // WHERE predicate references a variable passed through WITH without
    // renaming. The optimizer can push the filter below the WITH project
    // because the variable exists in the rows produced there.
    let p = parse_plan_opt("MATCH (u:User) WITH u WHERE u.id = 1 RETURN u");
    // Expected shape after pushdown: Project(RETURN) > Project(WITH) > Filter > Scan
    match p {
        Plan::Project { input: outer, .. } => match *outer {
            Plan::Project { input: inner, .. } => {
                assert!(
                    matches!(*inner, Plan::Filter { .. }),
                    "expected Filter pushed below WITH project, got {inner:?}"
                );
            }
            other => panic!("expected inner Project (WITH), got {other:?}"),
        },
        other => panic!("expected outer Project root, got {other:?}"),
    }
}
