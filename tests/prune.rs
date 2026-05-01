//! v0.10 projection pruning analysis tests.

use std::collections::HashSet;

use cypher_rs::*;

fn pq(src: &str) -> Plan {
    plan(&parse(src).unwrap()).unwrap()
}

fn s(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// ---- output_columns -----------------------------------------------------

#[test]
fn output_columns_scan_with_var() {
    let p = pq("MATCH (u:User) RETURN u");
    // Project root, single bare-variable item: column name is "u".
    assert_eq!(output_columns(&p), s(&["u"]));
}

#[test]
fn output_columns_project_aliases_replace_input_schema() {
    let p = pq("MATCH (u:User) RETURN u.name AS name, u.id AS id");
    assert_eq!(output_columns(&p), s(&["name", "id"]));
}

#[test]
fn output_columns_anonymous_project_item_contributes_nothing() {
    let p = pq("RETURN 1 + 2");
    // Project root, item has no alias and no underlying variable.
    // The set should be empty: there's a column at runtime, but we
    // can't name it from the AST alone.
    assert_eq!(output_columns(&p), s(&[]));
}

#[test]
fn output_columns_expand_exposes_dst_and_rel_var() {
    let p = pq("MATCH (u:User)-[r:KNOWS]->(v:User) RETURN v");
    // The Project at the root has a single item `v`, so the visible
    // output column is just `v`. The underlying Expand produces u, r,
    // v but the Project trims that.
    assert_eq!(output_columns(&p), s(&["v"]));
}

// ---- required_input_columns ----------------------------------------------

#[test]
fn required_input_columns_at_filter_includes_pred_vars() {
    let p = pq("MATCH (u:User) WHERE u.id = 1 RETURN u.name");
    // Walk to the Filter node and check what it demands of its input.
    // Optimizer pushes the filter below project, so the plan shape is
    // Project > Filter > Scan after `optimize`. We test on the
    // unoptimized plan: Project > Filter > Scan still.
    if let Plan::Project { input, .. } = &p {
        let demand = required_input_columns(input, &s(&[]));
        // Filter against `u.id = 1`. Demand on its input must include `u`.
        assert!(demand.contains("u"), "expected u in demand, got {demand:?}");
    } else {
        panic!("expected Project at root");
    }
}

#[test]
fn required_input_columns_at_project_keeps_only_referenced() {
    let p = pq("MATCH (u:User) RETURN u.name AS name");
    // Demand at the Project: outer demand asks for `name`. The
    // project's input must supply `u` (the variable referenced by
    // u.name) and not anything else.
    let demand = required_input_columns(&p, &s(&["name"]));
    assert_eq!(demand, s(&["u"]));
}

#[test]
fn required_input_columns_drops_unreferenced_project_items() {
    let p = pq("MATCH (u:User) RETURN u.name AS name, u.role AS role");
    // Outer demand only asks for `name`. The project should not
    // require `role`-side vars (which happen to be the same `u`
    // here, but in a real plan with multiple bound vars we'd see the
    // pruning).
    let demand = required_input_columns(&p, &s(&["name"]));
    assert_eq!(demand, s(&["u"]));
}

#[test]
fn required_input_columns_at_expand_swaps_dst_for_src() {
    // Build the expand sub-plan directly via the planner and walk it.
    let p = pq("MATCH (u:User)-[:KNOWS]->(v:User) RETURN v");
    // Plan shape: Project > Expand > Scan. We want the demand at the
    // Expand level given the project demands `v`.
    if let Plan::Project { input, .. } = &p {
        // input is the Expand. Outer demand for it is what the Project
        // recursively asks of its input: which is `v` (used in `v` projection).
        let project_demand = required_input_columns(&p, &s(&["v"]));
        assert!(project_demand.contains("v"));

        // Now ask the Expand what its input must supply, given demand `{v}`.
        // The Expand produces `v` (dst), so it should drop `v` from demand
        // but require `u` (src).
        let expand_demand = required_input_columns(input, &s(&["v"]));
        assert!(
            expand_demand.contains("u"),
            "expand should require src `u`, got {expand_demand:?}"
        );
        assert!(
            !expand_demand.contains("v"),
            "expand produces `v`; should not require it of input, got {expand_demand:?}"
        );
    } else {
        panic!("expected Project at root");
    }
}

#[test]
fn required_input_columns_leaf_ops_have_no_input() {
    let p = pq("MATCH (u:User) RETURN u");
    // Walk to the Scan and check it has no input demand.
    if let Plan::Project { input, .. } = &p {
        // input is the Scan.
        let demand = required_input_columns(input, &s(&["u"]));
        assert!(demand.is_empty(), "scan has no input, got {demand:?}");
    } else {
        panic!();
    }
}

#[test]
fn required_input_columns_at_sort_includes_key_vars() {
    let p = pq("MATCH (u:User) RETURN u.name AS name ORDER BY u.id");
    // Plan shape after lowering depends on clause order. The test
    // checks the plan tree contains a Sort whose required-input-
    // columns includes `u`.
    fn find_sort(p: &Plan) -> Option<&Plan> {
        match p {
            Plan::Sort { .. } => Some(p),
            Plan::Project { input, .. }
            | Plan::Filter { input, .. }
            | Plan::Limit { input, .. }
            | Plan::Skip { input, .. } => find_sort(input),
            _ => None,
        }
    }
    if let Some(sort_node) = find_sort(&p) {
        let demand = required_input_columns(sort_node, &s(&[]));
        assert!(
            demand.contains("u"),
            "sort key references u; demand should include u, got {demand:?}"
        );
    } else {
        panic!("expected a Sort node somewhere in the plan");
    }
}

#[test]
fn required_input_columns_passes_through_at_limit_skip() {
    let p = pq("MATCH (u:User) RETURN u LIMIT 5");
    fn find_limit(p: &Plan) -> Option<&Plan> {
        match p {
            Plan::Limit { .. } => Some(p),
            Plan::Project { input, .. } | Plan::Filter { input, .. } => find_limit(input),
            _ => None,
        }
    }
    if let Some(limit_node) = find_limit(&p) {
        // Demand passes through Limit: the same outer demand becomes
        // its input's demand (plus any vars in the count expression,
        // which here is a literal).
        let demand = required_input_columns(limit_node, &s(&["u"]));
        assert!(demand.contains("u"));
    } else {
        panic!("expected a Limit node");
    }
}

// ---- composition with existing analyses ---------------------------------

#[test]
fn output_columns_after_optimize_unchanged() {
    // Optimize is supposed to be semantics-preserving and shouldn't
    // change the output column set.
    let p = pq("MATCH (u:User) WHERE u.id = 1 RETURN u.name AS name");
    let opt = optimize(p.clone());
    assert_eq!(output_columns(&p), output_columns(&opt));
}
