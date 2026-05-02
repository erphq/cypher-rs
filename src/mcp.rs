//! MCP (Model Context Protocol) server for `cypher-rs`.
//!
//! Speaks the stdio MCP protocol (`protocolVersion 2024-11-05`) and
//! exposes the front-end pipeline as tools that any MCP client
//! (Claude Code, Cursor, Windsurf, etc.) can call.
//!
//! Compiled into the `cypher-mcp` binary when the crate is built with
//! `--features mcp`. The library API is unchanged; this module only
//! exists when the feature is on.
//!
//! Tools:
//! - `cypher_parse` - parse a query, return AST debug-print.
//! - `cypher_validate` - quick yes/no: does the query parse and pass
//!   semantic analysis?
//! - `cypher_analyze` - full semantic-analysis report (bindings,
//!   issues with severity / code / message).
//! - `cypher_plan` - logical plan (optionally optimized).
//! - `cypher_optimize` - logical plan before / after optimization.
//! - `cypher_explain` - full pipeline: parse -> analyze -> plan ->
//!   optimize -> cost -> columns. The headline tool.
//! - `cypher_cost` - cost estimate using `CardinalityCostModel`.
//! - `cypher_columns` - output columns + required input columns.

use crate::{
    analyze, estimate, optimize, output_columns, parse, plan, required_input_columns,
    sema::SemSeverity, CardinalityCostModel,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "cypher-rs";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// JSON-RPC / MCP envelope
// ---------------------------------------------------------------------------

/// Handle one line of JSON-RPC input. Returns `None` for notifications
/// (no response on the wire), `Some(json)` otherwise.
pub fn handle_request(line: &str) -> Option<String> {
    let request: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(error_response(
                Value::Null,
                -32700,
                &format!("Parse error: {e}"),
            ));
        }
    };

    let method = request["method"].as_str().unwrap_or("");
    let params = &request["params"];
    let id = request.get("id").cloned();
    let is_notification = id.as_ref().map(|v| v.is_null()).unwrap_or(true);
    let id_value = id.unwrap_or(Value::Null);

    match method {
        "initialize" => Some(success_response(
            id_value,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
            }),
        )),

        "notifications/initialized" | "initialized" | "notifications/cancelled" => None,

        "ping" => Some(success_response(id_value, json!({}))),

        "tools/list" => Some(success_response(
            id_value,
            json!({ "tools": tool_definitions() }),
        )),

        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match dispatch(name, &args) {
                Ok(result) => {
                    let text =
                        serde_json::to_string_pretty(&result).unwrap_or_else(|_| String::new());
                    Some(success_response(
                        id_value,
                        json!({
                            "content": [{ "type": "text", "text": text }],
                        }),
                    ))
                }
                Err(e) => Some(success_response(
                    id_value,
                    json!({
                        "content": [{ "type": "text", "text": e }],
                        "isError": true,
                    }),
                )),
            }
        }

        _ => {
            if is_notification {
                None
            } else {
                Some(error_response(
                    id_value,
                    -32601,
                    &format!("Method not found: {method}"),
                ))
            }
        }
    }
}

fn success_response(id: Value, result: Value) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    }))
    .unwrap_or_default()
}

fn error_response(id: Value, code: i32, message: &str) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    }))
    .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tool dispatch
// ---------------------------------------------------------------------------

fn dispatch(name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "cypher_parse" => tool_parse(args),
        "cypher_validate" => tool_validate(args),
        "cypher_analyze" => tool_analyze(args),
        "cypher_plan" => tool_plan(args),
        "cypher_optimize" => tool_optimize(args),
        "cypher_explain" => tool_explain(args),
        "cypher_cost" => tool_cost(args),
        "cypher_columns" => tool_columns(args),
        _ => Err(format!("Unknown tool: {name}")),
    }
}

fn get_query(args: &Value) -> Result<&str, String> {
    args["query"]
        .as_str()
        .ok_or_else(|| "Missing 'query' parameter (expected a string)".to_string())
}

fn issue_to_json(i: &crate::sema::SemIssue) -> Value {
    json!({
        "severity": match i.severity {
            SemSeverity::Error => "error",
            SemSeverity::Warning => "warning",
        },
        "code": i.code,
        "message": i.message,
    })
}

fn sorted_columns(set: std::collections::HashSet<String>) -> Vec<String> {
    set.into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn tool_parse(args: &Value) -> Result<Value, String> {
    let query = get_query(args)?;
    match parse(query) {
        Ok(q) => Ok(json!({
            "ok": true,
            "clause_count": q.clauses.len(),
            "ast": format!("{q:#?}"),
        })),
        Err(e) => Ok(json!({
            "ok": false,
            "error": format!("{e}"),
        })),
    }
}

fn tool_validate(args: &Value) -> Result<Value, String> {
    let query = get_query(args)?;
    match parse(query) {
        Ok(q) => {
            let report = analyze(&q);
            let errors: Vec<Value> = report.errors().map(issue_to_json).collect();
            let warning_count = report
                .issues
                .iter()
                .filter(|i| matches!(i.severity, SemSeverity::Warning))
                .count();
            Ok(json!({
                "valid": errors.is_empty(),
                "parse_ok": true,
                "analyze_ok": errors.is_empty(),
                "errors": errors,
                "warning_count": warning_count,
            }))
        }
        Err(e) => Ok(json!({
            "valid": false,
            "parse_ok": false,
            "analyze_ok": false,
            "errors": [json!({
                "severity": "error",
                "code": "parse_error",
                "message": format!("{e}"),
            })],
            "warning_count": 0,
        })),
    }
}

fn tool_analyze(args: &Value) -> Result<Value, String> {
    let query = get_query(args)?;
    let q = parse(query).map_err(|e| format!("parse error: {e}"))?;
    let report = analyze(&q);
    let mut bindings: Vec<String> = report.bindings.iter().cloned().collect();
    bindings.sort();
    let issues: Vec<Value> = report.issues.iter().map(issue_to_json).collect();
    Ok(json!({
        "bindings": bindings,
        "issues": issues,
        "has_errors": report.has_errors(),
    }))
}

fn tool_plan(args: &Value) -> Result<Value, String> {
    let query = get_query(args)?;
    let want_optimize = args
        .get("optimize")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let q = parse(query).map_err(|e| format!("parse error: {e}"))?;
    let p = plan(&q).map_err(|e| format!("plan error: {e:?}"))?;
    let final_plan = if want_optimize { optimize(p) } else { p };
    Ok(json!({
        "optimized": want_optimize,
        "plan": format!("{final_plan}"),
    }))
}

fn tool_optimize(args: &Value) -> Result<Value, String> {
    let query = get_query(args)?;
    let q = parse(query).map_err(|e| format!("parse error: {e}"))?;
    let p = plan(&q).map_err(|e| format!("plan error: {e:?}"))?;
    let before = format!("{p}");
    let optimized = optimize(p);
    let after = format!("{optimized}");
    Ok(json!({
        "before": before,
        "after": after,
        "changed": before != after,
    }))
}

fn tool_explain(args: &Value) -> Result<Value, String> {
    let query = get_query(args)?;
    let q = match parse(query) {
        Ok(q) => q,
        Err(e) => {
            return Ok(json!({
                "stage_failed": "parse",
                "error": format!("{e}"),
            }));
        }
    };
    let report = analyze(&q);
    let p = match plan(&q) {
        Ok(p) => p,
        Err(e) => {
            return Ok(json!({
                "stage_failed": "plan",
                "error": format!("{e:?}"),
                "analyze": {
                    "issues": report.issues.iter().map(issue_to_json).collect::<Vec<_>>(),
                    "has_errors": report.has_errors(),
                },
            }));
        }
    };
    let plan_before = format!("{p}");
    let optimized = optimize(p);
    let plan_after = format!("{optimized}");
    let est = estimate(&optimized, &CardinalityCostModel::default());
    let outs = sorted_columns(output_columns(&optimized));
    let needs = sorted_columns(required_input_columns(
        &optimized,
        &outs.iter().cloned().collect(),
    ));

    let mut bindings: Vec<String> = report.bindings.iter().cloned().collect();
    bindings.sort();

    Ok(json!({
        "parse": { "ok": true, "clause_count": q.clauses.len() },
        "analyze": {
            "bindings": bindings,
            "issues": report.issues.iter().map(issue_to_json).collect::<Vec<_>>(),
            "has_errors": report.has_errors(),
        },
        "plan": plan_before,
        "optimized_plan": plan_after,
        "optimizer_changed": plan_before != plan_after,
        "cost": {
            "cardinality": est.cardinality,
            "cost": est.cost,
            "model": "cardinality",
        },
        "output_columns": outs,
        "required_input_columns": needs,
    }))
}

fn tool_cost(args: &Value) -> Result<Value, String> {
    let query = get_query(args)?;
    let q = parse(query).map_err(|e| format!("parse error: {e}"))?;
    let p = plan(&q).map_err(|e| format!("plan error: {e:?}"))?;
    let p = optimize(p);
    let est = estimate(&p, &CardinalityCostModel::default());
    Ok(json!({
        "cardinality": est.cardinality,
        "cost": est.cost,
        "model": "cardinality",
        "plan": format!("{p}"),
    }))
}

fn tool_columns(args: &Value) -> Result<Value, String> {
    let query = get_query(args)?;
    let q = parse(query).map_err(|e| format!("parse error: {e}"))?;
    let p = plan(&q).map_err(|e| format!("plan error: {e:?}"))?;
    let p = optimize(p);
    let outs = output_columns(&p);
    let outs_sorted = sorted_columns(outs.clone());
    let needs = sorted_columns(required_input_columns(&p, &outs));
    Ok(json!({
        "output_columns": outs_sorted,
        "required_input_columns": needs,
    }))
}

// ---------------------------------------------------------------------------
// Tool definitions for `tools/list`
// ---------------------------------------------------------------------------

fn query_prop() -> Value {
    json!({ "type": "string", "description": "An openCypher query string." })
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "cypher_parse",
            "description": "Parse an openCypher query and return a debug print of the AST plus the clause count. Errors come back as { ok: false, error }.",
            "inputSchema": {
                "type": "object",
                "properties": { "query": query_prop() },
                "required": ["query"],
            }
        }),
        json!({
            "name": "cypher_validate",
            "description": "Quick yes/no: does the query parse and pass semantic analysis with no Error-severity issues? Returns { valid, parse_ok, analyze_ok, errors[], warning_count }.",
            "inputSchema": {
                "type": "object",
                "properties": { "query": query_prop() },
                "required": ["query"],
            }
        }),
        json!({
            "name": "cypher_analyze",
            "description": "Run semantic analysis on the query and return the bindings introduced by MATCH / OPTIONAL MATCH plus every issue (severity, code, message).",
            "inputSchema": {
                "type": "object",
                "properties": { "query": query_prop() },
                "required": ["query"],
            }
        }),
        json!({
            "name": "cypher_plan",
            "description": "Build the logical plan and return its tree-pretty-printed form. Pass `optimize: true` to apply the predicate-pushdown / projection-pruning rewriter before returning.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": query_prop(),
                    "optimize": { "type": "boolean", "description": "Apply optimizer rewrites before returning the plan.", "default": false },
                },
                "required": ["query"],
            }
        }),
        json!({
            "name": "cypher_optimize",
            "description": "Show the plan before and after the optimizer runs to fixpoint. Useful for inspecting which rewrites fire on a given query.",
            "inputSchema": {
                "type": "object",
                "properties": { "query": query_prop() },
                "required": ["query"],
            }
        }),
        json!({
            "name": "cypher_explain",
            "description": "Headline tool. Run the full pipeline (parse -> analyze -> plan -> optimize -> cost -> columns) and return every stage's output in one structured response. Use this when you want one call instead of six.",
            "inputSchema": {
                "type": "object",
                "properties": { "query": query_prop() },
                "required": ["query"],
            }
        }),
        json!({
            "name": "cypher_cost",
            "description": "Estimate the cost of the optimized plan using the default `CardinalityCostModel`. The cost is unitless - compare plans with each other, do not compare across models.",
            "inputSchema": {
                "type": "object",
                "properties": { "query": query_prop() },
                "required": ["query"],
            }
        }),
        json!({
            "name": "cypher_columns",
            "description": "Return the output columns the optimized plan produces and the input columns its leaves require. Useful when wiring the plan into an executor that materializes only referenced bindings.",
            "inputSchema": {
                "type": "object",
                "properties": { "query": query_prop() },
                "required": ["query"],
            }
        }),
    ]
}
