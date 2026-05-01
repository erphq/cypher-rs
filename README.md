<div align="center">

# `cypher-rs`

### openCypher front-end in Rust

**Lex. Parse. Validate. Plan. Storage-agnostic.**

[![license](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![status](https://img.shields.io/badge/status-pre--v0-orange.svg)](#roadmap)
[![rust](https://img.shields.io/badge/rust-stable-orange.svg)](#install)

</div>

A standalone openCypher front-end in pure Rust: lexer, parser, AST,
semantic analyzer, and logical plan generator. No storage. No executor.
No opinion about how nodes and edges are laid out on disk. Drop it into
any Rust graph-DB-shaped project that needs Cypher input without taking
on `libcypher-parser` as a C dependency.

> **The thesis.** Every embedded graph DB ends up reimplementing the
> same Cypher front-end, badly. The parser and the planner are
> separable from storage and execution - there's no good reason for
> each graph store to ship its own. This crate is the front-end,
> alone, done well, no batteries.

---

## ✦ Why a standalone front-end

There are good Cypher implementations behind the parser:

- **Neo4j** - closed-source, Java, JVM-only, embedded use is awkward.
- **`libcypher-parser`** - C, used by libgraph, requires linking against C.
- **Memgraph / RedisGraph** - embedded inside the database; not reusable.
- **Hand-rolled parsers** - every embedded graph DB project, every
  five years.

What's missing: a pure-Rust, library-grade, MIT-licensed Cypher
front-end with a clean separation between parsing, semantic analysis,
logical planning, and physical execution. `cypher-rs` is that piece. It
stops where storage starts.

## ✦ Scope

| Stage | What | Status |
|---|---|---|
| Lexer | tokens for openCypher 9 grammar | partial (v0.2) |
| Parser | concrete syntax tree | partial (v0.2: MATCH/OPTIONAL MATCH/WHERE/RETURN/ORDER BY/LIMIT/SKIP, list literals, IN) |
| AST lowering | symbol table, variable binding | partial (v0.3) |
| Semantic analysis | scope / label / rel-type checks | partial (v0.3 - type checks deferred) |
| Logical plan | algebra: scan · expand · filter · project · agg | planned |
| Plan rewriter | predicate pushdown, projection pruning | planned |
| Cost model | pluggable trait; default = cardinality-only | planned |
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

## ✦ Why standalone matters

- **No `libcypher-parser` dependency.** Pure Rust; builds anywhere
  Rust builds. No `bindgen`, no `pkg-config`, no system C library.
- **No executor coupling.** Plug into Sled, RocksDB, FFS, in-memory,
  remote - the crate doesn't care.
- **Reusable across deployments.** Embedded graph DB, server-side
  graph DB, OLAP graph engine - same front-end.
- **Inspectable plans.** The logical plan is data, not code. Print it,
  serialize it, optimize it, send it across a wire.

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
