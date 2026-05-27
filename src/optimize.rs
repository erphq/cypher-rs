//! Logical-plan optimizer (v0.6 / v0.11).
//!
//! Predicate pushdown moves a `Filter` as far down the tree as its
//! predicate's variable references allow, on the principle that
//! filtering early shrinks the rows that flow through later operators.
//!
//! Push directions:
//!   - `Filter(Project(input, exprs), pred)` -> `Project(Filter(input, pred), exprs)`
//!     when `pred` doesn't reference any project alias.
//!   - `Filter(Sort(input, keys), pred)` -> `Sort(Filter(input, pred), keys)`
//!     (always safe; predicate evaluation doesn't depend on order).
//!   - `Filter(Cartesian(l, r), pred)` -> `Cartesian(Filter(l, pred), r)`
//!     (or symmetric) when `pred` only references vars bound on one
//!     side.
//!   - `Filter(Expand(input, ..., rel_var, dst), pred)` ->
//!     `Expand(Filter(input, pred), ..., rel_var, dst)` when `pred`
//!     doesn't reference `rel_var` or `dst` (the vars Expand
//!     introduces). This avoids expanding rows that will be filtered
//!     immediately after.
//!
//! Push *blockers*: we don't push through `Limit`, `Skip`, or
//! `Optional`, because doing so changes which rows are seen.
//!
//! `optimize` runs the rewrite to a fixpoint. The transformation is
//! semantics-preserving: `eval(plan) == eval(optimize(plan))`.

use std::collections::HashSet;

use crate::ast::*;
use crate::plan::{Plan, ProjectExpr};

/// Apply pushdown rewrites until the plan stops changing.
pub fn optimize(plan: Plan) -> Plan {
    let mut current = plan;
    loop {
        let next = pass(current.clone());
        if next == current {
            return current;
        }
        current = next;
    }
}

fn pass(plan: Plan) -> Plan {
    let with_children = descend(plan);
    apply_local(with_children)
}

fn descend(plan: Plan) -> Plan {
    match plan {
        Plan::Filter { input, pred } => Plan::Filter {
            input: Box::new(pass(*input)),
            pred,
        },
        Plan::Project { input, exprs } => Plan::Project {
            input: Box::new(pass(*input)),
            exprs,
        },
        Plan::Sort { input, keys } => Plan::Sort {
            input: Box::new(pass(*input)),
            keys,
        },
        Plan::Skip { input, count } => Plan::Skip {
            input: Box::new(pass(*input)),
            count,
        },
        Plan::Limit { input, count } => Plan::Limit {
            input: Box::new(pass(*input)),
            count,
        },
        Plan::Expand {
            input,
            src,
            rel_var,
            rel_types,
            direction,
            dst,
            dst_label,
        } => Plan::Expand {
            input: Box::new(pass(*input)),
            src,
            rel_var,
            rel_types,
            direction,
            dst,
            dst_label,
        },
        Plan::Cartesian { left, right } => Plan::Cartesian {
            left: Box::new(pass(*left)),
            right: Box::new(pass(*right)),
        },
        Plan::Optional { input, optional } => Plan::Optional {
            input: Box::new(pass(*input)),
            optional: Box::new(pass(*optional)),
        },
        leaf @ (Plan::Empty | Plan::Scan { .. }) => leaf,
    }
}

fn apply_local(plan: Plan) -> Plan {
    match plan {
        Plan::Filter { input, pred } => try_push_filter(*input, pred),
        other => other,
    }
}

fn try_push_filter(input: Plan, pred: Expr) -> Plan {
    match input {
        Plan::Project { input: pi, exprs } => {
            // Push through Project unless the predicate references an alias
            // that the Project introduces.
            let aliases = project_aliases(&exprs);
            let used = used_vars(&pred);
            if used.is_disjoint(&aliases) {
                Plan::Project {
                    input: Box::new(try_push_filter(*pi, pred)),
                    exprs,
                }
            } else {
                Plan::Filter {
                    input: Box::new(Plan::Project { input: pi, exprs }),
                    pred,
                }
            }
        }
        Plan::Sort { input: si, keys } => Plan::Sort {
            input: Box::new(try_push_filter(*si, pred)),
            keys,
        },
        Plan::Cartesian { left, right } => {
            let used = used_vars(&pred);
            let left_vars = bound_vars(&left);
            let right_vars = bound_vars(&right);
            if !used.is_empty() && used.is_subset(&left_vars) {
                Plan::Cartesian {
                    left: Box::new(try_push_filter(*left, pred)),
                    right,
                }
            } else if !used.is_empty() && used.is_subset(&right_vars) {
                Plan::Cartesian {
                    left,
                    right: Box::new(try_push_filter(*right, pred)),
                }
            } else {
                Plan::Filter {
                    input: Box::new(Plan::Cartesian { left, right }),
                    pred,
                }
            }
        }
        Plan::Expand {
            input: ei,
            src,
            rel_var,
            rel_types,
            direction,
            dst,
            dst_label,
        } => {
            // Push below the Expand when pred doesn't reference the
            // vars Expand introduces (rel_var, dst). Those vars don't
            // exist in the input rows, so pushing is safe.
            let expand_introduces: HashSet<String> = [rel_var.as_deref(), dst.as_deref()]
                .into_iter()
                .flatten()
                .map(str::to_owned)
                .collect();
            if used_vars(&pred).is_disjoint(&expand_introduces) {
                Plan::Expand {
                    input: Box::new(try_push_filter(*ei, pred)),
                    src,
                    rel_var,
                    rel_types,
                    direction,
                    dst,
                    dst_label,
                }
            } else {
                Plan::Filter {
                    input: Box::new(Plan::Expand {
                        input: ei,
                        src,
                        rel_var,
                        rel_types,
                        direction,
                        dst,
                        dst_label,
                    }),
                    pred,
                }
            }
        }
        // Don't push through Limit / Skip / Optional / leaves.
        other => Plan::Filter {
            input: Box::new(other),
            pred,
        },
    }
}

// --- helpers -------------------------------------------------------------

fn project_aliases(exprs: &[ProjectExpr]) -> HashSet<String> {
    exprs.iter().filter_map(|e| e.alias.clone()).collect()
}

fn used_vars(expr: &Expr) -> HashSet<String> {
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

fn bound_vars(plan: &Plan) -> HashSet<String> {
    let mut out = HashSet::new();
    walk_bound(plan, &mut out);
    out
}

fn walk_bound(plan: &Plan, out: &mut HashSet<String>) {
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
            walk_bound(input, out);
            if let Some(v) = rel_var {
                out.insert(v.clone());
            }
            if let Some(v) = dst {
                out.insert(v.clone());
            }
        }
        Plan::Filter { input, .. } | Plan::Sort { input, .. } => walk_bound(input, out),
        Plan::Project { input, exprs } => {
            walk_bound(input, out);
            for e in exprs {
                if let Some(a) = &e.alias {
                    out.insert(a.clone());
                }
            }
        }
        Plan::Skip { input, .. } | Plan::Limit { input, .. } => walk_bound(input, out),
        Plan::Cartesian { left, right } => {
            walk_bound(left, out);
            walk_bound(right, out);
        }
        Plan::Optional { input, optional } => {
            walk_bound(input, out);
            walk_bound(optional, out);
        }
    }
}
