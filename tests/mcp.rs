//! Integration tests for the `mcp` module.
//!
//! Compiled only when the `mcp` feature is on; otherwise CI without
//! the feature still passes (we just skip these). Run with:
//!     cargo test --features mcp --test mcp

#![cfg(feature = "mcp")]

use cypher_rs::mcp::handle_request;
use serde_json::{json, Value};

fn req(method: &str, id: u64, params: Value) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    }))
    .unwrap()
}

fn notif(method: &str) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "method": method,
    }))
    .unwrap()
}

fn call(name: &str, args: Value) -> Value {
    let raw = handle_request(&req(
        "tools/call",
        1,
        json!({ "name": name, "arguments": args }),
    ))
    .expect("tools/call always responds");
    serde_json::from_str(&raw).expect("valid JSON")
}

fn call_text(name: &str, args: Value) -> Value {
    let resp = call(name, args);
    let text = resp["result"]["content"][0]["text"]
        .as_str()
        .expect("content[0].text is a string");
    serde_json::from_str(text).expect("tool returns JSON inside text")
}

// ---------- protocol envelope ----------

#[test]
fn initialize_returns_protocol_version_and_server_info() {
    let raw = handle_request(&req(
        "initialize",
        1,
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "tester", "version": "0.1" },
        }),
    ))
    .unwrap();
    let v: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(v["result"]["serverInfo"]["name"], "cypher-rs");
    assert!(v["result"]["capabilities"]["tools"].is_object());
}

#[test]
fn notifications_initialized_yields_no_response() {
    assert!(handle_request(&notif("notifications/initialized")).is_none());
}

#[test]
fn ping_returns_empty_result() {
    let raw = handle_request(&req("ping", 2, json!({}))).unwrap();
    let v: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["result"], json!({}));
}

#[test]
fn tools_list_returns_all_eight_tools_with_object_schemas() {
    let raw = handle_request(&req("tools/list", 3, json!({}))).unwrap();
    let v: Value = serde_json::from_str(&raw).unwrap();
    let tools = v["result"]["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 8, "expected 8 tools, got {}", tools.len());

    let expected: std::collections::HashSet<&str> = [
        "cypher_parse",
        "cypher_validate",
        "cypher_analyze",
        "cypher_plan",
        "cypher_optimize",
        "cypher_explain",
        "cypher_cost",
        "cypher_columns",
    ]
    .into_iter()
    .collect();
    let got: std::collections::HashSet<&str> =
        tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(got, expected);

    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object", "tool {}", t["name"]);
        let req_arr = t["inputSchema"]["required"].as_array().unwrap();
        assert!(
            req_arr.iter().any(|r| r == "query"),
            "tool {} should require query",
            t["name"]
        );
    }
}

#[test]
fn malformed_json_yields_parse_error() {
    let raw = handle_request("{not json").unwrap();
    let v: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["error"]["code"], -32700);
}

#[test]
fn unknown_method_yields_method_not_found() {
    let raw = handle_request(&req("totally/made/up", 9, json!({}))).unwrap();
    let v: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["error"]["code"], -32601);
}

#[test]
fn unknown_tool_yields_is_error_content() {
    let resp = call(
        "cypher_nonexistent",
        json!({ "query": "MATCH (n) RETURN n" }),
    );
    assert_eq!(resp["result"]["isError"], true);
}

#[test]
fn missing_query_argument_yields_is_error_content() {
    let resp = call("cypher_parse", json!({}));
    assert_eq!(resp["result"]["isError"], true);
}

// ---------- tool behavior ----------

const Q_OK: &str = "MATCH (u:User) WHERE u.id = $uid RETURN u.name";
const Q_OK_COMPLEX: &str =
    "MATCH (u:User)-[:FOLLOWS]->(f:User) WHERE u.id = $uid RETURN f.name AS name LIMIT 10";
const Q_BAD_SYNTAX: &str = "MATCH (u:User WHERE";

#[test]
fn parse_ok_query_returns_clause_count_and_ast() {
    let v = call_text("cypher_parse", json!({ "query": Q_OK }));
    assert_eq!(v["ok"], true);
    assert_eq!(v["clause_count"], 3);
    assert!(v["ast"].as_str().unwrap().contains("Match"));
}

#[test]
fn parse_bad_syntax_returns_ok_false_with_error() {
    let v = call_text("cypher_parse", json!({ "query": Q_BAD_SYNTAX }));
    assert_eq!(v["ok"], false);
    assert!(!v["error"].as_str().unwrap().is_empty());
}

#[test]
fn validate_clean_query_returns_valid_true() {
    let v = call_text("cypher_validate", json!({ "query": Q_OK }));
    assert_eq!(v["valid"], true);
    assert_eq!(v["parse_ok"], true);
    assert_eq!(v["analyze_ok"], true);
}

#[test]
fn validate_unparseable_query_returns_parse_error() {
    let v = call_text("cypher_validate", json!({ "query": Q_BAD_SYNTAX }));
    assert_eq!(v["valid"], false);
    assert_eq!(v["parse_ok"], false);
    assert_eq!(v["errors"][0]["code"], "parse_error");
}

#[test]
fn analyze_returns_bindings_for_match_pattern() {
    let v = call_text("cypher_analyze", json!({ "query": Q_OK_COMPLEX }));
    let bindings: Vec<&str> = v["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_str().unwrap())
        .collect();
    assert!(bindings.contains(&"u"));
    assert!(bindings.contains(&"f"));
    assert!(v["issues"].is_array());
}

#[test]
fn plan_returns_tree_string() {
    let v = call_text("cypher_plan", json!({ "query": Q_OK_COMPLEX }));
    assert_eq!(v["optimized"], false);
    let plan = v["plan"].as_str().unwrap();
    assert!(plan.contains("Scan") || plan.contains("Limit"));
}

#[test]
fn plan_with_optimize_flag_marks_response() {
    let v = call_text(
        "cypher_plan",
        json!({ "query": Q_OK_COMPLEX, "optimize": true }),
    );
    assert_eq!(v["optimized"], true);
}

#[test]
fn optimize_returns_before_and_after() {
    let v = call_text("cypher_optimize", json!({ "query": Q_OK_COMPLEX }));
    assert!(!v["before"].as_str().unwrap().is_empty());
    assert!(!v["after"].as_str().unwrap().is_empty());
    assert!(v["changed"].is_boolean());
}

#[test]
fn explain_returns_every_pipeline_stage() {
    let v = call_text("cypher_explain", json!({ "query": Q_OK_COMPLEX }));
    assert_eq!(v["parse"]["ok"], true);
    assert!(v["analyze"]["bindings"].is_array());
    assert!(v["plan"].is_string());
    assert!(v["optimized_plan"].is_string());
    assert!(v["cost"]["cost"].as_f64().unwrap() >= 0.0);
    assert!(v["output_columns"].is_array());
    assert!(v["required_input_columns"].is_array());
}

#[test]
fn explain_on_parse_failure_reports_stage_failed() {
    let v = call_text("cypher_explain", json!({ "query": Q_BAD_SYNTAX }));
    assert_eq!(v["stage_failed"], "parse");
    assert!(v["error"].is_string());
}

#[test]
fn cost_returns_cardinality_and_cost() {
    let v = call_text("cypher_cost", json!({ "query": Q_OK_COMPLEX }));
    assert!(v["cardinality"].as_f64().unwrap() >= 0.0);
    assert!(v["cost"].as_f64().unwrap() >= 0.0);
    assert_eq!(v["model"], "cardinality");
}

#[test]
fn columns_returns_output_and_required_input() {
    let v = call_text("cypher_columns", json!({ "query": Q_OK_COMPLEX }));
    assert!(v["output_columns"].is_array());
    assert!(v["required_input_columns"].is_array());
}
