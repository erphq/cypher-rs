//! Projection pruning analysis (v0.10).
//!
//! Two functions answer the column-tracking questions executors need
//! to materialize only what's referenced:
//!
//! - [`output_columns`] - what variables a plan's output rows
//!   contain. For `Project`, this is the set of aliases (or the
//!   underlying variable name when an item is a bare `Variable` with
//!   no alias). For other ops, it's the bindings the op produces or
//!   passes through.
//! - [`required_input_columns`] - given that the operators above this
//!   plan reference `outer_demand`, what variables must the plan's
//!   immediate input supply? Executors call this recursively to
//!   compute per-op input schemas without changing the plan algebra.
//!
//! No rewrites. No plan-tree changes. Pure analysis: pluggable
//! storage layers compute demand from the plan and decide what to
//! materialize.

use std::collections::HashSet;

use crate::ast::*;
use crate::plan::{Plan, ProjectExpr, SortKey};

/// Variables present in each row at the output of `plan`.
///
/// For `Project`, the columns are the aliases; if a `ProjectExpr`
/// has no alias and its expression is a bare `Variable(v)`, the
/// column is `v`. Anonymous projection items (e.g. `RETURN 1 + 2`
/// with no `AS`) contribute no column to this set.
///
/// For all other ops, the output schema is the same as the
/// underlying bindings the op exposes.
pub fn output_columns(plan: &Plan) -> HashSet<String> {
    let mut out = HashSet::new();
    walk_output(plan, &mut out);
    out
}

fn walk_output(plan: &Plan, out: &mut HashSet<String>) {
    match plan {
        Plan::Empty => {}
        Plan::Scan { var, .. } => {
            if let Some(v) = var {
                out.insert(v.clone());
            }
        }
        Plan::Expand {
            input,
            rel_var,
            dst,
            ..
        } => {
            walk_output(input, out);
            if let Some(v) = rel_var {
                out.insert(v.clone());
            }
            if let Some(v) = dst {
                out.insert(v.clone());
            }
        }
        Plan::Filter { input, .. }
        | Plan::Sort { input, .. }
        | Plan::Skip { input, .. }
        | Plan::Limit { input, .. } => walk_output(input, out),
        Plan::Project { exprs, .. } => {
            // Projects replace the output schema. Collect each item's
            // visible name: alias if present, else the bare Variable
            // name, else nothing (anonymous).
            for e in exprs {
                if let Some(name) = visible_name(e) {
                    out.insert(name);
                }
            }
        }
        Plan::Cartesian { left, right } => {
            walk_output(left, out);
            walk_output(right, out);
        }
        Plan::Optional { input, optional } => {
            walk_output(input, out);
            walk_output(optional, out);
        }
    }
}

fn visible_name(e: &ProjectExpr) -> Option<String> {
    if let Some(a) = &e.alias {
        return Some(a.clone());
    }
    match &e.expr {
        Expr::Variable(v) => Some(v.clone()),
        _ => None,
    }
}

/// Variables that the **input** of `plan` must supply so the plan
/// can satisfy `outer_demand` (the columns operators above reference).
///
/// For leaf ops (`Empty`, `Scan`), the result is empty; there is no
/// input. For `Project`, the input must supply every variable
/// referenced by any project expression that contributes to a column
/// the outer scope demands; aliases introduced by the project are
/// stripped.
pub fn required_input_columns(plan: &Plan, outer_demand: &HashSet<String>) -> HashSet<String> {
    match plan {
        Plan::Empty | Plan::Scan { .. } => HashSet::new(),
        Plan::Filter { pred, .. } => union(outer_demand, &used_vars_expr(pred)),
        Plan::Sort { keys, .. } => {
            let mut acc = outer_demand.clone();
            for k in keys {
                acc.extend(used_vars_expr(&k.expr));
            }
            acc
        }
        Plan::Skip { count, .. } | Plan::Limit { count, .. } => {
            // Skip / Limit don't reference row-bindings (count is
            // typically a literal or a parameter). Pass demand through.
            let mut acc = outer_demand.clone();
            acc.extend(used_vars_expr(count));
            acc
        }
        Plan::Project { exprs, .. } => {
            // The input must supply every variable referenced by any
            // project item whose visible name is in outer_demand,
            // plus every variable referenced by anonymous items
            // (which are still evaluated even if not consumed by a
            // demand set).
            let mut acc = HashSet::new();
            for e in exprs {
                let name = visible_name(e);
                let referenced = match &name {
                    Some(n) => outer_demand.is_empty() || outer_demand.contains(n),
                    None => true,
                };
                if referenced {
                    acc.extend(used_vars_expr(&e.expr));
                }
            }
            acc
        }
        Plan::Expand {
            src, rel_var, dst, ..
        } => {
            // Expand produces rel_var and dst on top of its input.
            // The input must supply demand minus those, plus src.
            let mut acc: HashSet<String> = outer_demand
                .iter()
                .filter(|v| rel_var.as_ref() != Some(*v) && dst.as_ref() != Some(*v))
                .cloned()
                .collect();
            if let Some(s) = src {
                acc.insert(s.clone());
            }
            acc
        }
        Plan::Cartesian { .. } | Plan::Optional { .. } => {
            // Both branches see the same outer demand, restricted to
            // variables that branch's subtree actually exposes.
            // For input-of-self purposes, the immediate input is the
            // root: callers typically recurse into `left` / `right` /
            // `optional` directly with split demand.
            outer_demand.clone()
        }
    }
}

fn union(a: &HashSet<String>, b: &HashSet<String>) -> HashSet<String> {
    let mut out = a.clone();
    out.extend(b.iter().cloned());
    out
}

fn used_vars_expr(expr: &Expr) -> HashSet<String> {
    let mut out = HashSet::new();
    walk_expr(expr, &mut out);
    out
}

fn walk_expr(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Variable(v) => {
            out.insert(v.clone());
        }
        Expr::Property { base, .. } => walk_expr(base, out),
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, out);
            walk_expr(rhs, out);
        }
        Expr::Unary { operand, .. } => walk_expr(operand, out),
        Expr::List(items) => {
            for i in items {
                walk_expr(i, out);
            }
        }
        Expr::Map(entries) => {
            for (_k, v) in entries {
                walk_expr(v, out);
            }
        }
        Expr::Literal(_) | Expr::Param(_) => {}
    }
}

// SortKey is referenced from the doctest only; suppress the
// "unused import" warning when feature flags evolve.
#[allow(dead_code)]
fn _sort_key_anchor(_k: &SortKey) {}
