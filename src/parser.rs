use pest::iterators::Pair;
use pest::Parser as _;
use pest_derive::Parser;

use crate::ast::*;
use crate::error::ParseError;

#[derive(Parser)]
#[grammar = "cypher.pest"]
pub(crate) struct CypherParser;

/// Parse an openCypher query string into a [`Query`] AST.
///
/// See the crate root for supported features and limitations.
pub fn parse(input: &str) -> Result<Query, ParseError> {
    let mut pairs = CypherParser::parse(Rule::query, input)?;
    let query_pair = pairs
        .next()
        .ok_or_else(|| ParseError::Unexpected("empty parse".into()))?;

    let mut clauses = Vec::new();
    for inner in query_pair.into_inner() {
        match inner.as_rule() {
            Rule::EOI => continue,
            Rule::match_clause => clauses.push(Clause::Match(walk_match(inner)?)),
            Rule::where_clause => clauses.push(Clause::Where(walk_clause_expr(inner)?)),
            Rule::with_clause => clauses.push(Clause::With(walk_return(inner)?)),
            Rule::return_clause => clauses.push(Clause::Return(walk_return(inner)?)),
            Rule::order_by_clause => clauses.push(Clause::OrderBy(walk_order_by(inner)?)),
            Rule::limit_clause => clauses.push(Clause::Limit(walk_clause_expr(inner)?)),
            Rule::skip_clause => clauses.push(Clause::Skip(walk_clause_expr(inner)?)),
            r => return Err(unexpected("clause", r)),
        }
    }
    Ok(Query { clauses })
}

// --- clauses --------------------------------------------------------------

fn walk_clause_expr(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    let inner = first_operand(pair, "clause expr")?;
    walk_expr(inner)
}

fn walk_match(pair: Pair<Rule>) -> Result<MatchClause, ParseError> {
    let mut patterns = Vec::new();
    let mut optional = false;
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::optional_kw => optional = true,
            Rule::pattern_list => {
                for p in inner.into_inner() {
                    patterns.push(walk_pattern(p)?);
                }
            }
            _ => {}
        }
    }
    Ok(MatchClause { optional, patterns })
}

fn walk_order_by(pair: Pair<Rule>) -> Result<Vec<OrderItem>, ParseError> {
    let mut items = Vec::new();
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::order_items {
            for oi in inner.into_inner() {
                items.push(walk_order_item(oi)?);
            }
        }
    }
    Ok(items)
}

fn walk_order_item(pair: Pair<Rule>) -> Result<OrderItem, ParseError> {
    let mut iter = pair.into_inner();
    let expr_pair = iter
        .next()
        .ok_or_else(|| ParseError::Unexpected("order_item: empty".into()))?;
    let expr = walk_expr(expr_pair)?;
    let mut desc = false;
    for d in iter {
        if d.as_rule() == Rule::order_dir {
            desc = d.as_str().eq_ignore_ascii_case("DESC");
        }
    }
    Ok(OrderItem { expr, desc })
}

fn walk_return(pair: Pair<Rule>) -> Result<ReturnClause, ParseError> {
    let mut items = Vec::new();
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::return_items {
            for ri in inner.into_inner() {
                items.push(walk_return_item(ri)?);
            }
        }
    }
    Ok(ReturnClause { items })
}

fn walk_return_item(pair: Pair<Rule>) -> Result<ReturnItem, ParseError> {
    let mut iter = pair.into_inner();
    let expr_pair = iter
        .next()
        .ok_or_else(|| ParseError::Unexpected("return_item: empty".into()))?;
    let expr = walk_expr(expr_pair)?;
    let mut alias = None;
    for a in iter {
        if a.as_rule() == Rule::alias {
            for ai in a.into_inner() {
                if ai.as_rule() == Rule::ident {
                    alias = Some(ai.as_str().to_string());
                }
            }
        }
    }
    Ok(ReturnItem { expr, alias })
}

// --- patterns -------------------------------------------------------------

fn walk_pattern(pair: Pair<Rule>) -> Result<Pattern, ParseError> {
    let mut iter = pair.into_inner();
    let anchor = walk_node(
        iter.next()
            .ok_or_else(|| ParseError::Unexpected("pattern: missing anchor".into()))?,
    )?;
    let mut chain = Vec::new();
    for c in iter {
        if c.as_rule() == Rule::rel_chain {
            let mut ci = c.into_inner();
            let rel = walk_rel(
                ci.next()
                    .ok_or_else(|| ParseError::Unexpected("rel_chain: missing rel".into()))?,
            )?;
            let node = walk_node(
                ci.next()
                    .ok_or_else(|| ParseError::Unexpected("rel_chain: missing node".into()))?,
            )?;
            chain.push(RelChain { rel, node });
        }
    }
    Ok(Pattern { anchor, chain })
}

fn walk_node(pair: Pair<Rule>) -> Result<NodePattern, ParseError> {
    let mut var = None;
    let mut labels = Vec::new();
    let mut properties = Vec::new();
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::ident => var = Some(inner.as_str().to_string()),
            Rule::labels => {
                for l in inner.into_inner() {
                    if l.as_rule() == Rule::label {
                        for li in l.into_inner() {
                            if li.as_rule() == Rule::ident {
                                labels.push(li.as_str().to_string());
                            }
                        }
                    }
                }
            }
            Rule::map_literal => {
                properties = walk_map_entries(inner)?;
            }
            _ => {}
        }
    }
    Ok(NodePattern {
        var,
        labels,
        properties,
    })
}

fn walk_rel(pair: Pair<Rule>) -> Result<RelPattern, ParseError> {
    let inner = first_inner(pair, "rel_pattern")?;
    let direction = match inner.as_rule() {
        Rule::rel_left => Direction::Incoming,
        Rule::rel_right => Direction::Outgoing,
        Rule::rel_undirected => Direction::Undirected,
        r => return Err(unexpected("rel direction", r)),
    };

    let mut var = None;
    let mut types = Vec::new();
    let mut properties = Vec::new();
    for d in inner.into_inner() {
        if d.as_rule() == Rule::rel_detail {
            for di in d.into_inner() {
                match di.as_rule() {
                    Rule::ident => var = Some(di.as_str().to_string()),
                    Rule::rel_types => {
                        for t in di.into_inner() {
                            if t.as_rule() == Rule::ident {
                                types.push(t.as_str().to_string());
                            }
                        }
                    }
                    Rule::map_literal => {
                        properties = walk_map_entries(di)?;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(RelPattern {
        var,
        direction,
        types,
        properties,
    })
}

/// Walk a `map_literal` rule's children and return the (key, value) pairs.
fn walk_map_entries(pair: Pair<Rule>) -> Result<Vec<(String, Expr)>, ParseError> {
    let mut out = Vec::new();
    for entry in pair.into_inner() {
        if entry.as_rule() != Rule::map_entry {
            continue;
        }
        let mut iter = entry.into_inner();
        let key_pair = iter
            .next()
            .ok_or_else(|| ParseError::Unexpected("map_entry: missing key".into()))?;
        let val_pair = iter
            .next()
            .ok_or_else(|| ParseError::Unexpected("map_entry: missing value".into()))?;
        let key = key_pair.as_str().to_string();
        let val = walk_expr(val_pair)?;
        out.push((key, val));
    }
    Ok(out)
}

// --- expressions ----------------------------------------------------------

fn walk_expr(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    match pair.as_rule() {
        Rule::expr => walk_expr(first_inner(pair, "expr")?),
        Rule::or_expr => walk_left_assoc_no_op(pair, BinOp::Or),
        Rule::and_expr => walk_left_assoc_no_op(pair, BinOp::And),
        Rule::not_op => {
            let inner = first_operand(pair, "not")?;
            Ok(Expr::Unary {
                op: UnOp::Not,
                operand: Box::new(walk_expr(inner)?),
            })
        }
        Rule::cmp_expr => walk_cmp(pair),
        Rule::add_expr => walk_left_assoc_with_op(pair, |s| match s {
            "+" => Some(BinOp::Add),
            "-" => Some(BinOp::Sub),
            _ => None,
        }),
        Rule::mul_expr => walk_left_assoc_with_op(pair, |s| match s {
            "*" => Some(BinOp::Mul),
            "/" => Some(BinOp::Div),
            "%" => Some(BinOp::Mod),
            _ => None,
        }),
        Rule::neg_op => {
            let inner = first_inner(pair, "neg")?;
            Ok(Expr::Unary {
                op: UnOp::Neg,
                operand: Box::new(walk_expr(inner)?),
            })
        }
        Rule::postfix_expr => walk_postfix(pair),
        Rule::paren_expr => walk_expr(first_inner(pair, "paren")?),
        Rule::var_ref => {
            let inner = first_inner(pair, "var_ref")?;
            Ok(Expr::Variable(inner.as_str().to_string()))
        }
        Rule::param => Ok(Expr::Param(
            pair.as_str().trim_start_matches('$').to_string(),
        )),
        Rule::integer => {
            let s = pair.as_str();
            Ok(Expr::Literal(Literal::Int(
                s.parse::<i64>()
                    .map_err(|_| ParseError::InvalidInt(s.into()))?,
            )))
        }
        Rule::float => {
            let s = pair.as_str();
            Ok(Expr::Literal(Literal::Float(
                s.parse::<f64>()
                    .map_err(|_| ParseError::InvalidFloat(s.into()))?,
            )))
        }
        Rule::string_lit => {
            let s = pair.as_str();
            let inner = if s.len() >= 2 { &s[1..s.len() - 1] } else { "" };
            Ok(Expr::Literal(Literal::String(inner.to_string())))
        }
        Rule::bool_lit => Ok(Expr::Literal(Literal::Bool(
            pair.as_str().eq_ignore_ascii_case("true"),
        ))),
        Rule::null_lit => Ok(Expr::Literal(Literal::Null)),
        Rule::list_lit => {
            let mut items = Vec::new();
            for item in pair.into_inner() {
                items.push(walk_expr(item)?);
            }
            Ok(Expr::List(items))
        }
        Rule::map_literal => {
            let entries = walk_map_entries(pair)?;
            Ok(Expr::Map(entries))
        }
        r => Err(unexpected("walk_expr", r)),
    }
}

/// `or_expr` and `and_expr` interleave operand - kw_or/kw_and - operand - ...
/// We walk operands and skip the keyword tokens.
fn walk_left_assoc_no_op(pair: Pair<Rule>, op: BinOp) -> Result<Expr, ParseError> {
    let mut acc: Option<Expr> = None;
    for inner in pair.into_inner() {
        if is_kw(&inner) {
            continue;
        }
        let next = walk_expr(inner)?;
        acc = Some(match acc {
            None => next,
            Some(lhs) => Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(next),
            },
        });
    }
    acc.ok_or_else(|| ParseError::Unexpected("left_assoc: no operands".into()))
}

/// `add_expr` and `mul_expr` interleave operand, op, operand, op, ...
fn walk_left_assoc_with_op<F>(pair: Pair<Rule>, op_for: F) -> Result<Expr, ParseError>
where
    F: Fn(&str) -> Option<BinOp>,
{
    let mut iter = pair.into_inner();
    let first = iter
        .next()
        .ok_or_else(|| ParseError::Unexpected("arith: empty".into()))?;
    let mut acc = walk_expr(first)?;
    while let Some(op_pair) = iter.next() {
        let op = op_for(op_pair.as_str())
            .ok_or_else(|| ParseError::Unexpected(format!("arith op: {}", op_pair.as_str())))?;
        let rhs_pair = iter
            .next()
            .ok_or_else(|| ParseError::Unexpected("arith: missing rhs".into()))?;
        let rhs = walk_expr(rhs_pair)?;
        acc = Expr::Binary {
            op,
            lhs: Box::new(acc),
            rhs: Box::new(rhs),
        };
    }
    Ok(acc)
}

fn walk_cmp(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    let mut iter = pair.into_inner();
    let lhs_pair = iter
        .next()
        .ok_or_else(|| ParseError::Unexpected("cmp: empty".into()))?;
    let lhs = walk_expr(lhs_pair)?;
    let tail = match iter.next() {
        Some(p) => p,
        None => return Ok(lhs),
    };
    match tail.as_rule() {
        Rule::cmp_op_tail => {
            let mut tail_iter = tail.into_inner();
            let op_pair = tail_iter
                .next()
                .ok_or_else(|| ParseError::Unexpected("cmp_op_tail: missing op".into()))?;
            let rhs_pair = tail_iter
                .next()
                .ok_or_else(|| ParseError::Unexpected("cmp_op_tail: missing rhs".into()))?;
            let op = match op_pair.as_str() {
                "=" => BinOp::Eq,
                "<>" => BinOp::Neq,
                "<" => BinOp::Lt,
                "<=" => BinOp::Lte,
                ">" => BinOp::Gt,
                ">=" => BinOp::Gte,
                s => return Err(ParseError::Unexpected(format!("cmp op: {s}"))),
            };
            let rhs = walk_expr(rhs_pair)?;
            Ok(Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            })
        }
        Rule::in_op_tail => {
            let rhs_pair = first_operand(tail, "in_op_tail")?;
            let rhs = walk_expr(rhs_pair)?;
            Ok(Expr::Binary {
                op: BinOp::In,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            })
        }
        Rule::starts_with_tail => {
            let rhs_pair = first_operand(tail, "starts_with_tail")?;
            let rhs = walk_expr(rhs_pair)?;
            Ok(Expr::Binary {
                op: BinOp::StartsWith,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            })
        }
        Rule::ends_with_tail => {
            let rhs_pair = first_operand(tail, "ends_with_tail")?;
            let rhs = walk_expr(rhs_pair)?;
            Ok(Expr::Binary {
                op: BinOp::EndsWith,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            })
        }
        Rule::contains_tail => {
            let rhs_pair = first_operand(tail, "contains_tail")?;
            let rhs = walk_expr(rhs_pair)?;
            Ok(Expr::Binary {
                op: BinOp::Contains,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            })
        }
        Rule::is_null_tail => Ok(Expr::Unary {
            op: UnOp::IsNull,
            operand: Box::new(lhs),
        }),
        Rule::is_not_null_tail => Ok(Expr::Unary {
            op: UnOp::IsNotNull,
            operand: Box::new(lhs),
        }),
        r => Err(unexpected("cmp tail", r)),
    }
}

fn walk_postfix(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    let mut iter = pair.into_inner();
    let first = iter
        .next()
        .ok_or_else(|| ParseError::Unexpected("postfix: empty".into()))?;
    let mut acc = walk_expr(first)?;
    for p in iter {
        if p.as_rule() == Rule::prop_access {
            let key = first_inner(p, "prop_access")?;
            acc = Expr::Property {
                base: Box::new(acc),
                key: key.as_str().to_string(),
            };
        }
    }
    Ok(acc)
}

// --- helpers --------------------------------------------------------------

fn first_inner<'a>(pair: Pair<'a, Rule>, what: &'static str) -> Result<Pair<'a, Rule>, ParseError> {
    pair.into_inner()
        .next()
        .ok_or_else(|| ParseError::Unexpected(format!("{what}: missing inner")))
}

/// Return the first child that isn't a `kw_*` token.
fn first_operand<'a>(
    pair: Pair<'a, Rule>,
    what: &'static str,
) -> Result<Pair<'a, Rule>, ParseError> {
    for inner in pair.into_inner() {
        if !is_kw(&inner) {
            return Ok(inner);
        }
    }
    Err(ParseError::Unexpected(format!("{what}: missing operand")))
}

fn is_kw(p: &Pair<Rule>) -> bool {
    matches!(
        p.as_rule(),
        Rule::kw_match
            | Rule::kw_optional
            | Rule::kw_where
            | Rule::kw_return
            | Rule::kw_as
            | Rule::kw_order
            | Rule::kw_by
            | Rule::kw_asc
            | Rule::kw_desc
            | Rule::kw_limit
            | Rule::kw_skip
            | Rule::kw_with
            | Rule::kw_and
            | Rule::kw_or
            | Rule::kw_not
            | Rule::kw_in
            | Rule::kw_true
            | Rule::kw_false
            | Rule::kw_null
            | Rule::kw_is
            | Rule::kw_starts
            | Rule::kw_ends
            | Rule::kw_contains
    )
}

fn unexpected(ctx: &str, rule: Rule) -> ParseError {
    ParseError::Unexpected(format!("{ctx}: {rule:?}"))
}
