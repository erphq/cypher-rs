//! Logical query plan and AST-to-plan lowering.
//!
//! v0.4 scope: a small algebra of relational/graph operators —
//! `Scan`, `Expand`, `Filter`, `Project`, `Sort`, `Limit`, `Skip` —
//! and a straightforward lowering that walks the parsed [`Query`]
//! and emits a tree of [`Plan`] nodes. No optimization. No cost
//! model. No OPTIONAL MATCH. No multi-pattern cartesian product.
//! Those are v0.5+.
//!
//! The plan is data, not code. Print it, serialize it, optimize it,
//! send it across a wire. See the [`std::fmt::Display`] impl for
//! the indented tree rendering.

use std::fmt;

use crate::ast::*;

/// Logical plan operator. Plans are trees; every operator carries
/// its input(s).
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    /// Empty input. Used for queries with no `MATCH` (e.g. `RETURN 1`).
    Empty,
    /// Scan all nodes (optionally filtered by `label`), binding them
    /// to `var`.
    Scan {
        var: Option<String>,
        label: Option<String>,
    },
    /// Extend each row by following a relationship from `src` to `dst`.
    Expand {
        input: Box<Plan>,
        src: Option<String>,
        rel_var: Option<String>,
        rel_types: Vec<String>,
        direction: Direction,
        dst: Option<String>,
        dst_label: Option<String>,
    },
    /// Keep only rows where `pred` evaluates to true.
    Filter { input: Box<Plan>, pred: Expr },
    /// Replace columns with the projected expressions.
    Project {
        input: Box<Plan>,
        exprs: Vec<ProjectExpr>,
    },
    /// Sort rows by `keys` (left-to-right priority).
    Sort {
        input: Box<Plan>,
        keys: Vec<SortKey>,
    },
    /// Discard the first `count` rows.
    Skip { input: Box<Plan>, count: Expr },
    /// Keep at most `count` rows after any prior `Skip`.
    Limit { input: Box<Plan>, count: Expr },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectExpr {
    pub expr: Expr,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SortKey {
    pub expr: Expr,
    pub desc: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// The query had no clauses at all.
    EmptyQuery,
    /// v0.4 lowers exactly one MATCH per query.
    MultipleMatch,
    /// v0.4 lowers exactly one pattern per MATCH.
    MultiPattern,
    /// v0.4 doesn't lower OPTIONAL MATCH.
    OptionalMatchUnsupported,
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlanError::EmptyQuery => f.write_str("plan: empty query"),
            PlanError::MultipleMatch => {
                f.write_str("plan: multiple MATCH clauses are not supported in v0.4")
            }
            PlanError::MultiPattern => {
                f.write_str("plan: multiple patterns in one MATCH are not supported in v0.4")
            }
            PlanError::OptionalMatchUnsupported => {
                f.write_str("plan: OPTIONAL MATCH is not supported in v0.4")
            }
        }
    }
}

impl std::error::Error for PlanError {}

/// Lower a parsed query into a logical plan tree.
pub fn plan(query: &Query) -> Result<Plan, PlanError> {
    if query.clauses.is_empty() {
        return Err(PlanError::EmptyQuery);
    }

    let mut plan = Plan::Empty;
    let mut project: Option<Vec<ProjectExpr>> = None;
    let mut sort: Option<Vec<SortKey>> = None;
    let mut skip: Option<Expr> = None;
    let mut limit: Option<Expr> = None;
    let mut saw_match = false;

    for clause in &query.clauses {
        match clause {
            Clause::Match(m) => {
                if m.optional {
                    return Err(PlanError::OptionalMatchUnsupported);
                }
                if saw_match {
                    return Err(PlanError::MultipleMatch);
                }
                saw_match = true;
                plan = lower_match(m)?;
            }
            Clause::Where(e) => {
                plan = Plan::Filter {
                    input: Box::new(plan),
                    pred: e.clone(),
                };
            }
            Clause::Return(r) => {
                project = Some(
                    r.items
                        .iter()
                        .map(|i| ProjectExpr {
                            expr: i.expr.clone(),
                            alias: i.alias.clone(),
                        })
                        .collect(),
                );
            }
            Clause::OrderBy(items) => {
                sort = Some(
                    items
                        .iter()
                        .map(|i| SortKey {
                            expr: i.expr.clone(),
                            desc: i.desc,
                        })
                        .collect(),
                );
            }
            Clause::Skip(e) => skip = Some(e.clone()),
            Clause::Limit(e) => limit = Some(e.clone()),
        }
    }

    // Stack post-RETURN clauses on top: project → sort → skip → limit.
    if let Some(exprs) = project {
        plan = Plan::Project {
            input: Box::new(plan),
            exprs,
        };
    }
    if let Some(keys) = sort {
        plan = Plan::Sort {
            input: Box::new(plan),
            keys,
        };
    }
    if let Some(count) = skip {
        plan = Plan::Skip {
            input: Box::new(plan),
            count,
        };
    }
    if let Some(count) = limit {
        plan = Plan::Limit {
            input: Box::new(plan),
            count,
        };
    }

    Ok(plan)
}

fn lower_match(m: &MatchClause) -> Result<Plan, PlanError> {
    if m.patterns.is_empty() {
        return Err(PlanError::EmptyQuery);
    }
    if m.patterns.len() > 1 {
        return Err(PlanError::MultiPattern);
    }
    let pattern = &m.patterns[0];
    let mut current = Plan::Scan {
        var: pattern.anchor.var.clone(),
        label: pattern.anchor.labels.first().cloned(),
    };
    let mut head = pattern.anchor.var.clone();
    for chain in &pattern.chain {
        current = Plan::Expand {
            input: Box::new(current),
            src: head.clone(),
            rel_var: chain.rel.var.clone(),
            rel_types: chain.rel.types.clone(),
            direction: chain.rel.direction,
            dst: chain.node.var.clone(),
            dst_label: chain.node.labels.first().cloned(),
        };
        head = chain.node.var.clone();
    }
    Ok(current)
}

// --- pretty-printing -------------------------------------------------------

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_plan(self, f, 0, true)
    }
}

fn write_plan(plan: &Plan, f: &mut fmt::Formatter<'_>, depth: usize, root: bool) -> fmt::Result {
    let indent = "    ".repeat(depth.saturating_sub(1));
    let lead = if root { "" } else { "└── " };
    write!(f, "{indent}{lead}")?;
    match plan {
        Plan::Empty => writeln!(f, "Empty")?,
        Plan::Scan { var, label } => writeln!(
            f,
            "Scan {{ var: {}, label: {} }}",
            opt_str(var.as_deref()),
            opt_str(label.as_deref()),
        )?,
        Plan::Expand {
            input,
            src,
            rel_var,
            rel_types,
            direction,
            dst,
            dst_label,
        } => {
            writeln!(
                f,
                "Expand {{ src: {}, rel: {}, types: [{}], dir: {}, dst: {}, dst_label: {} }}",
                opt_str(src.as_deref()),
                opt_str(rel_var.as_deref()),
                rel_types.join(", "),
                direction_str(*direction),
                opt_str(dst.as_deref()),
                opt_str(dst_label.as_deref()),
            )?;
            write_plan(input, f, depth + 1, false)?;
        }
        Plan::Filter { input, pred } => {
            writeln!(f, "Filter {{ pred: {pred:?} }}")?;
            write_plan(input, f, depth + 1, false)?;
        }
        Plan::Project { input, exprs } => {
            let parts: Vec<String> = exprs
                .iter()
                .map(|e| match &e.alias {
                    Some(a) => format!("{:?} AS {a}", e.expr),
                    None => format!("{:?}", e.expr),
                })
                .collect();
            writeln!(f, "Project {{ exprs: [{}] }}", parts.join(", "))?;
            write_plan(input, f, depth + 1, false)?;
        }
        Plan::Sort { input, keys } => {
            let parts: Vec<String> = keys
                .iter()
                .map(|k| format!("{:?} {}", k.expr, if k.desc { "DESC" } else { "ASC" }))
                .collect();
            writeln!(f, "Sort {{ keys: [{}] }}", parts.join(", "))?;
            write_plan(input, f, depth + 1, false)?;
        }
        Plan::Skip { input, count } => {
            writeln!(f, "Skip {{ count: {count:?} }}")?;
            write_plan(input, f, depth + 1, false)?;
        }
        Plan::Limit { input, count } => {
            writeln!(f, "Limit {{ count: {count:?} }}")?;
            write_plan(input, f, depth + 1, false)?;
        }
    }
    Ok(())
}

fn opt_str(s: Option<&str>) -> String {
    match s {
        Some(v) => v.to_string(),
        None => "_".to_string(),
    }
}

fn direction_str(d: Direction) -> &'static str {
    match d {
        Direction::Outgoing => "->",
        Direction::Incoming => "<-",
        Direction::Undirected => "--",
    }
}
