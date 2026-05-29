<div align="center">

# `cypher-rs`

### openCypher front-end in Rust

**Lex. Parse. Validate. Plan. Storage-agnostic.**

![tests](https://img.shields.io/badge/tests-130%20passing-green)

[![license](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![status](https://img.shields.io/badge/status-pre--v0-orange.svg)](#roadmap)
[![rust](https://img.shields.io/badge/rust-stable-orange.svg)](#install)

</div>

A standalone openCypher front-end in pure Rust: lexer, parser, AST,
semantic analyzer, and logical plan generator. No storage. No executor.
No opinion about how nodes and edges are laid out on disk. Drop it into
any Rust graph-DB-shaped project that needs Cypher input without taking
on `libcypher-parser` as a C dependency.

## Why

Every embedded graph DB ends up reimplementing the same Cypher front-end. The parser and the planner are separable from storage and execution, and there's no good reason each graph store should ship its own. The existing implementations sit behind the parser: Neo4j is closed-source Java, `libcypher-parser` is C and pulls in `bindgen` / `pkg-config` plumbing on every build, Memgraph and RedisGraph keep their parsers inside the database, and the rest are hand-rolled per project.

What's missing is a pure-Rust, library-grade, MIT-licensed Cypher front-end with a clean separation between parsing, semantic analysis, logical planning, and execution. `cypher-rs` is that piece. It stops where storage starts.

## ✦ Scope

| Stage | What | Status |
|---|---|---|
| Lexer | tokens for openCypher 9 grammar | partial (v0.2) |
| Parser | concrete syntax tree | partial (v0.2: MATCH/OPTIONAL MATCH/WHERE/RETURN/ORDER BY/LIMIT/SKIP, list literals, IN) |
| AST lowering | symbol table, variable binding | partial (v0.3) |
| Semantic analysis | scope / label / rel-type checks | partial (v0.3 - type checks deferred) |
| Logical plan | algebra: scan · expand · filter · project · agg | partial (v0.4-v0.5) |
| Plan rewriter | predicate pushdown, projection-pruning analysis | partial (v0.6 pushdown, v0.10 prune analysis) |
| Cost model | pluggable trait; default = cardinality-only | partial (v0.7) |
| Diagnostics | `miette`-powered span errors | planned |

Not in scope: physical plan, storage adapter, executor, network
protocol, server.

## ✦ Usage

```rust
use cypher_rs::{parse, lower, plan, Schema};

let q = "
    MATCH (u:User)-[:FOLLOWS]->(f:User)
    WHERE u.id = $uid
    RETURN f.name AS name, f.created_at AS joined
    ORDER BY joined DESC
    LIMIT 10
";

let cst = parse(q)?;                       // concrete syntax tree
let ast = lower(&cst)?;                    // typed AST with bindings
let lp  = plan(&ast, &Schema::infer())?;   // logical plan
println!("{lp:#}");
```

Output (simplified):

```text
Limit { count: 10 }
└── Sort { keys: [joined DESC] }
    └── Project { exprs: [f.name AS name, f.created_at AS joined] }
        └── Filter { pred: u.id = $uid }
            └── Expand { src: u, rel: :FOLLOWS, dir: out, dst: f, label: User }
                └── Scan { var: u, label: User }
```

Plug your executor under that and you have a Cypher engine.

## ✦ MCP Server

`cypher-rs` ships a [Model Context Protocol](https://modelcontextprotocol.io/) server so any MCP-compatible agent ([Claude Code](https://docs.anthropic.com/en/docs/claude-code), [Cursor](https://cursor.sh/), [Windsurf](https://codeium.com/windsurf), [Continue](https://continue.dev/), …) can call the front-end as tools. Useful for "is this query valid? what's its plan? what does the optimizer change? what does it cost?" without hand-rolling a CLI.

The server is gated behind the `mcp` Cargo feature so the library's dep tree stays lean by default. Build the binary once:

```bash
cargo build --release --features mcp --bin cypher-mcp
```

Register it with Claude Code:

```bash
claude mcp add --scope user cypher-rs "$(pwd)/target/release/cypher-mcp"
claude mcp list   # cypher-rs: ... - ✓ Connected
```

For Cursor / Windsurf / any other MCP client, drop this into their `mcpServers` config:

```json
{
  "mcpServers": {
    "cypher-rs": {
      "command": "/abs/path/to/cypher-rs/target/release/cypher-mcp"
    }
  }
}
```

The server is stateless: every call takes a `query` string. No session state, no warm-up, sub-millisecond startup.

### Tools

| Tool | What it does |
|------|--------------|
| `cypher_parse` | Parse a query, return a debug-print of the AST + clause count. |
| `cypher_validate` | Quick yes/no: does the query parse and pass semantic analysis? |
| `cypher_analyze` | Full semantic-analysis report: bindings introduced by MATCH / OPTIONAL MATCH plus every issue (severity / code / message). |
| `cypher_plan` | Build the logical plan and return its tree-pretty-printed form. Pass `optimize: true` to apply the rewriter first. |
| `cypher_optimize` | Plan before / after the optimizer runs to fixpoint, plus a `changed` flag. Useful for inspecting which rewrites fire. |
| `cypher_explain` | **Headline tool.** Run the full pipeline (parse → analyze → plan → optimize → cost → columns) in one call. |
| `cypher_cost` | Cost estimate using `CardinalityCostModel`. Unitless — compare plans, do not compare across models. |
| `cypher_columns` | Output columns + required input columns for the optimized plan. |

### Using it

In Claude Code, after registering, talk to Claude in English:

> **You:** Use cypher-rs to validate this query and explain the plan it produces:
>
> ```cypher
> MATCH (u:User)-[:FOLLOWS]->(f:User)
> WHERE u.id = $uid
> RETURN f.name AS name, f.created_at AS joined
> ORDER BY joined DESC
> LIMIT 10
> ```
>
> *(Claude calls `cypher_explain` and reads back the plan tree, the optimizer's rewrite, the cardinality estimate, and the column set.)*
>
> **You:** Why does the optimizer not push the predicate below the Sort?
>
> *(Claude calls `cypher_optimize` to show the before/after, then reasons over it.)*

You can also force a tool ("use `cypher_validate`…") but normally describing the question in English is faster.

## ✦ What separation buys you

- **No `libcypher-parser` dependency.** Pure Rust; builds anywhere Rust builds. No `bindgen`, no `pkg-config`, no system C library.
- **No executor coupling.** Plug into Sled, RocksDB, FFS, in-memory, remote; the crate doesn't care.
- **Reusable across deployments.** Embedded graph DB, server-side graph DB, OLAP graph engine; same front-end.
- **Inspectable plans.** The logical plan is data, not code. Print it, serialize it, optimize it, send it across a wire.

## ✦ Design choices

- **Parser**: `pest` for v0 (PEG, easy to read, easy to evolve). May
  switch to `lalrpop` if benchmarks demand.
- **AST**: enum-heavy. No `Box<dyn Trait>` everywhere. If allocation
  patterns get hot, an arena layer is straightforward.
- **Errors**: `miette` for diagnostics with source spans. Cypher errors
  feel like Rust errors, with carets and context.
- **No `async`**: front-end is pure CPU work. Async belongs at the
  storage layer, not here.
- **Generic over schemas**: a `Schema` trait lets you provide whatever
  metadata you have (or nothing). Validation is opt-in.

## ✦ The algebra

Logical plans are built from a small algebra:

| Op | Inputs | Output |
|---|---|---|
| `Scan { var, label? }` | - | rows binding `var` |
| `Expand { src, rel, dir, dst, label? }` | rows | rows extended with `dst` |
| `Filter { pred }` | rows | rows where `pred` holds |
| `Project { exprs }` | rows | rows with new columns |
| `Aggregate { group_by, aggs }` | rows | grouped rows |
| `Sort { keys }` | rows | sorted rows |
| `Limit { offset, count }` | rows | bounded rows |
| `Union { lhs, rhs }` | rows × rows | concatenated rows |
| `Join { lhs, rhs, kind }` | rows × rows | joined rows |
| `OptionalExpand` | rows | rows with optional `dst` |
| `Create / Merge / SetProperty / Delete` | rows | mutations |

Every plan is a tree of these. Optimization rules are tree rewrites.

## ✦ Conformance

The bar is the [openCypher Technology Compatibility Kit](https://github.com/opencypher/openCypher/tree/master/tck).
v0 targets the parser-only subset. v1 targets ≥95% on the full TCK.

## ✦ Comparison

| | `cypher-rs` | `libcypher-parser` | hand-rolled | Neo4j embedded |
|---|---|---|---|---|
| Language | Rust | C | varies | Java |
| Pure | ✓ | ✗ (system dep) | ✓ | ✗ (JVM) |
| Logical plan | ✓ | ✗ (parser only) | ad-hoc | ✓ |
| Cost model | ✓ (pluggable) | ✗ | ad-hoc | ✓ (fixed) |
| License | MIT | Apache 2.0 | varies | GPLv3/commercial |
| TCK conformance | targeted | partial | varies | full |

## ✦ Integrations (planned)

- `cypher-rs-sled` - adapter for the [Sled](https://sled.rs) embedded KV store
- `cypher-rs-rocksdb` - adapter for RocksDB
- `cypher-rs-ffs` - adapter for the FFS embedded graph DB (closed)
- `cypher-rs-arrow` - vectorized executor over Apache Arrow

## ✦ Diagnostics

Errors carry source spans. A typo:

```text
error: unknown property `naem` on label `User`
   ╭─[query.cypher:3:14]
 3 │   RETURN u.naem
   ·              ─┬─
   ·               ╰── did you mean `name`?
   ╰────
```

## ✦ Non-goals

- **Physical execution.** That's the storage adapter's job.
- **Storage layer.** Not here.
- **Neo4j-specific extensions.** APOC, GDS, Bolt, internal catalogs -
  out of scope. The goal is portable openCypher, not Cypher-as-Neo4j.
- **Distributed planning.** A future crate, not this one.

## ✦ FAQ

**Q: Is `cypher-rs` faster than `libcypher-parser`?**
A: TBD. The goal is parity for v1, then optimization. Pure Rust gives
us LTO and inlining the C version doesn't have.

**Q: What about GQL (the new ISO standard)?**
A: openCypher is a strict subset of GQL. The plan is openCypher → GQL
delta tracking, with a feature flag.

**Q: Will this work in WebAssembly?**
A: Yes - pure Rust, no `unsafe`, no system deps. A `cypher-rs-wasm`
companion crate is on the roadmap.

**Q: Why pest and not lalrpop / chumsky / nom?**
A: Pest's PEG grammar tracks the openCypher EBNF closely; refactoring
the grammar is a textual operation. We may revisit if profiling shows
pest is the bottleneck.

**Q: Does this support `CALL` procedures?**
A: Built-in `CALL` is in scope; user-defined procedures (which are
backend-specific) are not.

## ✦ Roadmap

- [x] v0.0 - scaffold, design, scope
- [x] v0.1 - lexer + parser; MATCH / WHERE / RETURN / LIMIT / SKIP, expressions
- [x] v0.2 - OPTIONAL MATCH · ORDER BY (multi-key, ASC/DESC) · list literals · `IN`. Atomic `kw_*` rules with word-boundary checks for every keyword.
- [x] v0.3 - semantic analyzer (variable binding · scope check · optional schema-aware label / rel-type validation via `Schema` trait)
- [x] v0.4 - logical plan + algebra (Empty · Scan · Expand · Filter · Project · Sort · Skip · Limit) with indented tree pretty-print
- [x] v0.5 - multi-pattern Cartesian · multiple MATCH clauses · OPTIONAL MATCH (`Optional` outer-apply)
- [x] v0.6 - predicate pushdown optimizer (`optimize(plan)` to fixpoint; pushes through Project / Sort / Cartesian; respects Limit / Skip / Optional)
- [x] v0.7 - cost-model trait (`CostModel`) + `CardinalityCostModel` default + `estimate_cost(plan, model) -> f64`
- [x] v0.8 - map literals (`{key: value}`) as expressions and as node/rel pattern properties; planner desugars pattern-property maps into Filter operators
- [x] v0.9 - anonymous rel-binding synthesis (patterns like `(:User {id: 1})` and `[:KNOWS {since: 2020}]` now lower to a Filter against an internal `__node_N` / `__rel_N` binding instead of dropping the predicate). Projection pruning (column-set tracking) deferred to v0.10.
- [x] v0.10 - projection pruning analysis (`output_columns(plan)` + `required_input_columns(plan, outer_demand)` for column-set tracking; pure analysis, no plan-tree changes; executors use it to materialize only referenced bindings). `cypher-rs-sled` integration crate deferred to v0.11.
- [ ] v1.0 - openCypher TCK ≥ 95%; used in FFS

## ✦ Topics

`cypher` · `opencypher` · `graph-database` · `query-language` ·
`parser` · `rust` · `compiler` · `query-planner` · `embedded-database` ·
`pest` · `graph-algorithms`

## ✦ License

MIT - see [LICENSE](./LICENSE).
