//! Semantic analysis: variable binding and scope checking.
//!
//! v0.3 scope is intentionally narrow:
//!
//! 1. Walk every `MATCH` / `OPTIONAL MATCH` clause, collect every
//!    variable name introduced by node patterns and relationship
//!    detail brackets. The collected set is the binding scope of the
//!    query.
//! 2. Walk every expression in `WHERE` / `RETURN` / `ORDER BY` /
//!    `LIMIT` / `SKIP` and check that every `Expr::Variable` resolves
//!    to either a binding or a parameter (which is always external).
//! 3. Also flag references to labels and relationship types against
//!    an optional `Schema` - but only when the user provides one.
//!    Without a schema, the analyzer is silent on labels.
//!
//! Type checking, expression-level type inference, and physical
//! resolution are explicitly out of scope.

use std::collections::HashSet;

use crate::ast::*;

/// User-supplied metadata about the data the query will run against.
/// All methods default to "permissive" (everything is valid) so callers
/// can opt in to validation field-by-field.
pub trait Schema {
    fn has_label(&self, _label: &str) -> bool {
        true
    }
    fn has_rel_type(&self, _rel_type: &str) -> bool {
        true
    }
}

/// Schema impl that approves every label and rel-type. Used as the
/// default so analysis never fails on label/rel-type checks unless a
/// caller opts in to a stricter `Schema`.
pub struct PermissiveSchema;
impl Schema for PermissiveSchema {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemIssue {
    pub severity: SemSeverity,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnalysisReport {
    /// Variable names introduced by MATCH/OPTIONAL MATCH patterns.
    pub bindings: HashSet<String>,
    pub issues: Vec<SemIssue>,
}

impl AnalysisReport {
    pub fn errors(&self) -> impl Iterator<Item = &SemIssue> {
        self.issues
            .iter()
            .filter(|i| matches!(i.severity, SemSeverity::Error))
    }

    pub fn has_errors(&self) -> bool {
        self.errors().next().is_some()
    }
}

/// Analyze a parsed query. Uses [`PermissiveSchema`] which approves
/// every label and rel-type. For schema-aware validation, call
/// [`analyze_with`] and pass your own `Schema`.
pub fn analyze(query: &Query) -> AnalysisReport {
    analyze_with(query, &PermissiveSchema)
}

/// Analyze a parsed query against `schema`.
pub fn analyze_with<S: Schema + ?Sized>(query: &Query, schema: &S) -> AnalysisReport {
    let mut report = AnalysisReport::default();

    collect_bindings(query, &mut report.bindings);

    for clause in &query.clauses {
        check_clause(clause, &report.bindings, schema, &mut report.issues);
    }

    report
}

fn collect_bindings(query: &Query, out: &mut HashSet<String>) {
    for clause in &query.clauses {
        match clause {
            Clause::Match(m) => {
                for p in &m.patterns {
                    add_node_binding(&p.anchor, out);
                    for chain in &p.chain {
                        add_rel_binding(&chain.rel, out);
                        add_node_binding(&chain.node, out);
                    }
                }
            }
            Clause::With(w) => {
                for item in &w.items {
                    if let Some(alias) = &item.alias {
                        out.insert(alias.clone());
                    }
                }
            }
            Clause::Unwind { var, .. } => {
                out.insert(var.clone());
            }
            _ => {}
        }
    }
}

fn add_node_binding(n: &NodePattern, out: &mut HashSet<String>) {
    if let Some(v) = &n.var {
        out.insert(v.clone());
    }
}

fn add_rel_binding(r: &RelPattern, out: &mut HashSet<String>) {
    if let Some(v) = &r.var {
        out.insert(v.clone());
    }
}

fn check_clause<S: Schema + ?Sized>(
    clause: &Clause,
    bindings: &HashSet<String>,
    schema: &S,
    issues: &mut Vec<SemIssue>,
) {
    match clause {
        Clause::Match(m) => {
            for p in &m.patterns {
                check_node_pattern(&p.anchor, schema, issues);
                for chain in &p.chain {
                    check_rel_pattern(&chain.rel, schema, issues);
                    check_node_pattern(&chain.node, schema, issues);
                }
            }
        }
        Clause::Where(e) => check_expr(e, bindings, issues),
        Clause::Return(r) | Clause::With(r) => {
            for item in &r.items {
                check_expr(&item.expr, bindings, issues);
            }
        }
        Clause::OrderBy(items) => {
            for item in items {
                check_expr(&item.expr, bindings, issues);
            }
        }
        Clause::Limit(e) | Clause::Skip(e) => check_expr(e, bindings, issues),
        Clause::Unwind { expr, .. } => check_expr(expr, bindings, issues),
    }
}

fn check_node_pattern<S: Schema + ?Sized>(n: &NodePattern, schema: &S, issues: &mut Vec<SemIssue>) {
    for label in &n.labels {
        if !schema.has_label(label) {
            issues.push(SemIssue {
                severity: SemSeverity::Error,
                code: "unknown-label",
                message: format!("unknown label `{label}`"),
            });
        }
    }
}

fn check_rel_pattern<S: Schema + ?Sized>(r: &RelPattern, schema: &S, issues: &mut Vec<SemIssue>) {
    for ty in &r.types {
        if !schema.has_rel_type(ty) {
            issues.push(SemIssue {
                severity: SemSeverity::Error,
                code: "unknown-rel-type",
                message: format!("unknown relationship type `{ty}`"),
            });
        }
    }
}

fn check_expr(expr: &Expr, bindings: &HashSet<String>, issues: &mut Vec<SemIssue>) {
    match expr {
        Expr::Variable(name) => {
            if !bindings.contains(name) {
                issues.push(SemIssue {
                    severity: SemSeverity::Error,
                    code: "unbound-variable",
                    message: format!(
                        "unbound variable `{name}` (introduce it in a MATCH pattern, or use $name for a parameter)"
                    ),
                });
            }
        }
        Expr::Property { base, .. } => check_expr(base, bindings, issues),
        Expr::Binary { lhs, rhs, .. } => {
            check_expr(lhs, bindings, issues);
            check_expr(rhs, bindings, issues);
        }
        Expr::Unary { operand, .. } => check_expr(operand, bindings, issues),
        Expr::List(items) => {
            for item in items {
                check_expr(item, bindings, issues);
            }
        }
        Expr::Map(entries) => {
            for (_k, v) in entries {
                check_expr(v, bindings, issues);
            }
        }
        Expr::Literal(_) | Expr::Param(_) => {}
    }
}
