//! Cost model for logical plans (v0.7).
//!
//! Two pieces:
//!
//!   1. The [`CostModel`] trait. Users plug in whatever statistics
//!      they have - exact node counts from a catalog, sampled
//!      counts, or hand-tuned defaults. The trait carries methods
//!      for the three knobs that drive plan cost: scan cardinality,
//!      expand fan-out, and filter selectivity.
//!
//!   2. [`CardinalityCostModel`] - a permissive default that knows
//!      nothing concrete and applies simple constants. Useful for
//!      tests, demos, and "good enough until you have stats."
//!
//! Cost is estimated by [`estimate_cost`], which walks the plan tree
//! and returns a single `f64` - bigger is more expensive. The score
//! is unitless; what matters is comparing two plans against the same
//! model.
//!
//! The point of v0.7 is to give the optimizer something to score
//! against. Future passes (v0.8+) can try multiple rewrite candidates
//! and keep the cheapest.

use std::collections::HashMap;

use crate::ast::{Direction, Expr};
use crate::plan::Plan;

/// Pluggable cost model. All methods have permissive defaults so
/// implementors can override only what they have stats for.
pub trait CostModel {
    /// Estimated number of rows produced by `Scan { label }`.
    fn scan_cardinality(&self, _label: Option<&str>) -> f64 {
        1_000.0
    }

    /// Estimated fan-out per row when expanding along `rel_types` in
    /// `direction`. A fan-out of 1.0 means "exactly one neighbor on
    /// average."
    fn expand_fanout(&self, _rel_types: &[String], _direction: Direction) -> f64 {
        2.0
    }

    /// Estimated selectivity of `pred` - the fraction of rows that
    /// pass it. Must be in `[0.0, 1.0]`.
    fn filter_selectivity(&self, _pred: &Expr) -> f64 {
        0.1
    }
}

/// Cardinality-driven default cost model. No statistics - just
/// configurable defaults plus optional per-label / per-rel-type
/// overrides.
#[derive(Debug, Clone)]
pub struct CardinalityCostModel {
    /// Cardinality of an unlabeled `Scan`.
    pub default_node_count: f64,
    /// Per-label cardinality overrides.
    pub label_counts: HashMap<String, f64>,
    /// Default fan-out when no rel-type-specific value is known.
    pub default_fanout: f64,
    /// Per-rel-type fan-out overrides.
    pub rel_fanouts: HashMap<String, f64>,
    /// Default predicate selectivity.
    pub default_selectivity: f64,
}

impl Default for CardinalityCostModel {
    fn default() -> Self {
        Self {
            default_node_count: 10_000.0,
            label_counts: HashMap::new(),
            default_fanout: 5.0,
            rel_fanouts: HashMap::new(),
            default_selectivity: 0.1,
        }
    }
}

impl CardinalityCostModel {
    pub fn with_label(mut self, label: &str, count: f64) -> Self {
        self.label_counts.insert(label.to_string(), count);
        self
    }

    pub fn with_rel(mut self, rel: &str, fanout: f64) -> Self {
        self.rel_fanouts.insert(rel.to_string(), fanout);
        self
    }
}

impl CostModel for CardinalityCostModel {
    fn scan_cardinality(&self, label: Option<&str>) -> f64 {
        match label {
            Some(l) => self
                .label_counts
                .get(l)
                .copied()
                .unwrap_or(self.default_node_count),
            None => self.default_node_count,
        }
    }

    fn expand_fanout(&self, rel_types: &[String], _direction: Direction) -> f64 {
        if rel_types.is_empty() {
            return self.default_fanout;
        }
        // Average across listed rel-types - the planner doesn't yet
        // know which one a row will follow.
        let total: f64 = rel_types
            .iter()
            .map(|t| {
                self.rel_fanouts
                    .get(t)
                    .copied()
                    .unwrap_or(self.default_fanout)
            })
            .sum();
        total / rel_types.len() as f64
    }

    fn filter_selectivity(&self, _pred: &Expr) -> f64 {
        self.default_selectivity
    }
}

/// Per-operator estimate carrying both rows-out and accumulated cost.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    pub cardinality: f64,
    pub cost: f64,
}

/// Estimate the cost of `plan` against `model`. The returned `f64` is
/// unitless - compare plans, don't compare across models.
pub fn estimate_cost<M: CostModel + ?Sized>(plan: &Plan, model: &M) -> f64 {
    estimate(plan, model).cost
}

/// Same as [`estimate_cost`] but returns the full estimate including
/// estimated cardinality.
pub fn estimate<M: CostModel + ?Sized>(plan: &Plan, model: &M) -> Estimate {
    match plan {
        Plan::Empty => Estimate {
            cardinality: 1.0,
            cost: 0.0,
        },
        Plan::Scan { label, .. } => {
            let card = model.scan_cardinality(label.as_deref());
            Estimate {
                cardinality: card,
                cost: card,
            }
        }
        Plan::Expand {
            input,
            rel_types,
            direction,
            ..
        } => {
            let inp = estimate(input, model);
            let f = model.expand_fanout(rel_types, *direction);
            let card = inp.cardinality * f;
            Estimate {
                cardinality: card,
                cost: inp.cost + card,
            }
        }
        Plan::Filter { input, pred } => {
            let inp = estimate(input, model);
            let sel = model.filter_selectivity(pred).clamp(0.0, 1.0);
            Estimate {
                cardinality: inp.cardinality * sel,
                cost: inp.cost + inp.cardinality,
            }
        }
        Plan::Project { input, .. } => {
            let inp = estimate(input, model);
            Estimate {
                cardinality: inp.cardinality,
                cost: inp.cost + inp.cardinality,
            }
        }
        Plan::Sort { input, .. } => {
            let inp = estimate(input, model);
            let n = inp.cardinality.max(1.0);
            Estimate {
                cardinality: n,
                cost: inp.cost + n * n.ln().max(1.0),
            }
        }
        Plan::Limit { input, count } => {
            let inp = estimate(input, model);
            let lim = expr_as_f64(count).unwrap_or(inp.cardinality);
            Estimate {
                cardinality: lim.min(inp.cardinality),
                cost: inp.cost + lim.min(inp.cardinality),
            }
        }
        Plan::Skip { input, count } => {
            let inp = estimate(input, model);
            let skip = expr_as_f64(count).unwrap_or(0.0);
            Estimate {
                cardinality: (inp.cardinality - skip).max(0.0),
                cost: inp.cost + inp.cardinality,
            }
        }
        Plan::Cartesian { left, right } => {
            let l = estimate(left, model);
            let r = estimate(right, model);
            let card = l.cardinality * r.cardinality;
            Estimate {
                cardinality: card,
                cost: l.cost + r.cost + card,
            }
        }
        Plan::Optional { input, optional } => {
            let i = estimate(input, model);
            let o = estimate(optional, model);
            // Outer-apply: at least the input rows pass through; up
            // to the optional rows extend them.
            let card = i.cardinality.max(o.cardinality);
            Estimate {
                cardinality: card,
                cost: i.cost + o.cost + i.cardinality,
            }
        }
    }
}

fn expr_as_f64(e: &Expr) -> Option<f64> {
    match e {
        Expr::Literal(crate::ast::Literal::Int(n)) => Some(*n as f64),
        Expr::Literal(crate::ast::Literal::Float(f)) => Some(*f),
        _ => None,
    }
}
