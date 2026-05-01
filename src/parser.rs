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
            Rule::return_clause => clauses.push(Clause::Return(walk_return(inner)?)),
            Rule::limit_clause => clauses.push(Clause::Limit(walk_clause_expr(inner)?)),
            Rule::skip_clause => clauses.push(Clause::Skip(walk_clause_expr(inner)?)),
            r => return Err(unexpected("clause", r)),
        }
    }
    Ok(Query { clauses })
}

// --- clauses --------------------------------------------------------------

fn walk_clause_expr(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    let inner = first_inner(pair, "clause expr")?;
    walk_expr(inner)
}

fn walk_match(pair: Pair<Rule>) -> Result<MatchClause, ParseError> {
    let mut patterns = Vec::new();
    for inner in pair.into_inner() {
        if inner.as_rule() == Rule::pattern_list {
            for p in inner.into_inner() {
                patterns.push(walk_pattern(p)?);
            }
        }
    }
    Ok(MatchClause {
        optional: false,
        patterns,
    })
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
            _ => {}
        }
    }
    Ok(NodePattern { var, labels })
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
                    _ => {}
                }
            }
        }
    }
    Ok(RelPattern {
        var,
        direction,
        types,
    })
}

// --- expressions ----------------------------------------------------------

fn walk_expr(pair: Pair<Rule>) -> Result<Expr, ParseError> {
    match pair.as_rule() {
        Rule::expr => walk_expr(first_inner(pair, "expr")?),
        Rule::or_expr => walk_left_assoc_no_op(pair, BinOp::Or),
        Rule::and_expr => walk_left_assoc_no_op(pair, BinOp::And),
        Rule::not_op => {
            let inner = first_inner(pair, "not")?;
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
        r => Err(unexpected("walk_expr", r)),
    }
}

/// `or_expr` and `and_expr` have only operand children (the keyword
/// tokens aren't emitted by pest because they're literals, not rules).
fn walk_left_assoc_no_op(pair: Pair<Rule>, op: BinOp) -> Result<Expr, ParseError> {
    let mut iter = pair.into_inner();
    let first = iter
        .next()
        .ok_or_else(|| ParseError::Unexpected("left_assoc: empty".into()))?;
    let mut acc = walk_expr(first)?;
    for next in iter {
        let rhs = walk_expr(next)?;
        acc = Expr::Binary {
            op,
            lhs: Box::new(acc),
            rhs: Box::new(rhs),
        };
    }
    Ok(acc)
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
    let op_pair = match iter.next() {
        Some(p) => p,
        None => return Ok(lhs),
    };
    let rhs_pair = iter
        .next()
        .ok_or_else(|| ParseError::Unexpected("cmp: missing rhs".into()))?;
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

fn unexpected(ctx: &str, rule: Rule) -> ParseError {
    ParseError::Unexpected(format!("{ctx}: {rule:?}"))
}
