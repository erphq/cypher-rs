# Changelog

All notable changes to `cypher-rs` are documented here. The format
follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.0] - 2026-05-01

### Added
- Anonymous rel-binding synthesis. Patterns like `(:User {id: 1})`
  and `[:KNOWS {since: 2020}]` now synthesize an internal `__node_N`
  / `__rel_N` binding so their property predicates can lower to a
  `Filter` instead of being dropped. The lowered shape is
  `Project > Filter > Scan` (or `Filter > Expand`) with the
  filter's lhs referencing the synthesized var.
- Tests covering both synthesized-rel and synthesized-node paths,
  plus a guard that bare patterns like `(:User)` (no properties) do
  not introduce a binding.

### Changed
- `lower_pattern` threads a `Synth` counter through the chain so
  successive anonymous patterns get distinct fresh names without
  colliding.

### Fixed
- Lifts the v0.8 limitation: rel patterns with property maps but
  no user-given var no longer drop the predicate silently.

## [0.8.0] - 2026-05-01

### Added
- Map literals as expressions: `{key: value, ...}`. New `Expr::Map`
  variant; values can be any expression (literals, lists, params,
  variables, nested maps).
- Property maps on node and rel patterns:
  `(u:User {id: $uid})` and `[:KNOWS {since: 2020}]`. New
  `properties: Vec<(String, Expr)>` field on `NodePattern` and
  `RelPattern`.
- Planner desugaring: pattern-property maps lower to AND-chained
  `Filter` operators of the form `var.key = value`. Empty maps
  (`{}`) introduce no filter.
- Sema and optimizer walk `Expr::Map` values for unbound-variable
  checks and predicate pushdown.

### Known limitations
- Rel patterns with a property map but no bound var (e.g.
  `[:KNOWS {since: 2020}]`) drop the predicate because there is
  nothing to attach the filter to. Fixed in v0.9.

## [0.7.0] - 2026-05-01

### Added
- Cost-model trait. `CostModel` exposes three knobs:
  `scan_cardinality(label)`, `expand_fanout(rel_types, dir)`,
  `filter_selectivity(pred)`. All methods have permissive defaults
  so implementors override only what they have stats for.
- `CardinalityCostModel` default with per-label and per-rel-type
  overrides via `with_label()` / `with_rel()` builders.
- `estimate(plan, model) -> Estimate` and `estimate_cost(plan, model)
  -> f64` walk the plan tree. Per-op rules: Scan (card), Expand
  (card × fanout), Filter (× selectivity), Sort (n log n), Cartesian
  (l × r), Optional (max(l, r)), Limit caps, Skip subtracts.
- One integration test proves the v0.6 optimizer wins (pushdown
  lowers estimated cost on Filter + Cartesian).

## [0.6.0] - 2026-05-01

### Added
- Predicate pushdown optimizer. `optimize(plan)` runs to a fixpoint:
  - `Filter(Project) → Project(Filter)` when the predicate doesn't
    reference any project alias.
  - `Filter(Sort) → Sort(Filter)` always.
  - `Filter(Cartesian)` pushes into the side whose bound vars
    cover the predicate's used vars.
- Push blockers preserved by design: never through `Limit`, `Skip`,
  or `Optional` (would change which rows are seen).
- Helpers: `used_vars(expr)` walks an `Expr`; `bound_vars(plan)`
  walks a `Plan` collecting Scan / Expand / Project bindings.

## [0.5.0] - 2026-05-01

### Added
- Multi-pattern `MATCH` lowers to a left-deep `Cartesian` chain.
- Multiple top-level `MATCH` clauses also chain via `Cartesian`
  (no longer errors out as in v0.4).
- `OPTIONAL MATCH` lowers to `Optional` (outer-apply): for each
  input row, evaluate the optional plan; emit input rows with null
  bindings when the optional branch produces nothing.

### Changed
- `PlanError` simplified to two variants: `EmptyQuery` and
  `OptionalMatchWithoutAnchor`. The previous `MultipleMatch`,
  `MultiPattern`, `OptionalMatchUnsupported` variants are gone -
  their conditions now plan successfully.

## [0.4.0] - 2026-05-01

### Added
- Logical plan + AST-to-plan lowering. New `plan` module with the
  algebra: `Empty` / `Scan` / `Expand` / `Filter` / `Project` /
  `Sort` / `Skip` / `Limit`.
- `plan(query) -> Result<Plan, PlanError>` walks the AST. Stack
  order: MATCH / WHERE / RETURN inline; post-RETURN clauses stack
  project → sort → skip → limit.
- Multi-hop relationship chains thread `src` correctly:
  `(a)-[:R]->(b)-[:S]->(c)` becomes `Expand(b, S, c)` over
  `Expand(a, R, b)`.
- `Display` impl prints the indented tree shown in the README.
- `examples/demo.rs` prints the kitchen-sink query's plan.

## [0.3.0] - 2026-05-01

### Added
- Semantic analyzer. New `sema` module:
  - `Schema` trait (permissive by default); users opt in to
    label / rel-type validation by impl'ing `has_label()` /
    `has_rel_type()`.
  - `analyze(query)` and `analyze_with(query, schema)` return
    `AnalysisReport { bindings, issues }`.
  - Three rule codes: `unbound-variable` (error),
    `unknown-label` (error), `unknown-rel-type` (error).

## [0.2.0] - 2026-05-01

### Added
- Grammar growth: `OPTIONAL MATCH`, `ORDER BY` (multi-key, ASC /
  DESC), list literals (`[1, 2, 3]`), `IN` operator.
- New `BinOp::In`, `Expr::List`, `Clause::OrderBy(Vec<OrderItem>)`,
  `OrderItem`.

### Changed
- Refactored every keyword in the grammar to atomic `kw_*` rules
  with a word-boundary check (`!(ASCII_ALPHANUMERIC | "_")`).
  Fixed a class of bugs where bare literals like `^"OR"` happily
  matched the `OR` prefix of `ORDER` and broke the parse.

## [0.1.0] - 2026-04-30

### Added
- Initial release. Pest-based pure-Rust openCypher front-end.
- Grammar: `MATCH`, `WHERE`, `RETURN`, `LIMIT`, `SKIP`.
- Patterns: nodes (multi-label), in / out / undirected
  relationships with types.
- Expressions: literals (int / float / string / bool / null),
  variables, parameters (`$name`), property access (incl.
  nested), comparisons, `AND` / `OR` / `NOT`, arithmetic,
  parentheses, aliases (`AS`).
- `ast` module with full AST types; `thiserror`-based `ParseError`.
- 18 integration tests + 1 doctest. CI: build + test +
  `clippy -D warnings` + `fmt --check`.

[Unreleased]: https://github.com/erphq/cypher-rs/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/erphq/cypher-rs/releases/tag/v0.7.0
[0.6.0]: https://github.com/erphq/cypher-rs/releases/tag/v0.6.0
[0.5.0]: https://github.com/erphq/cypher-rs/releases/tag/v0.5.0
[0.4.0]: https://github.com/erphq/cypher-rs/releases/tag/v0.4.0
[0.3.0]: https://github.com/erphq/cypher-rs/releases/tag/v0.3.0
[0.2.0]: https://github.com/erphq/cypher-rs/releases/tag/v0.2.0
[0.1.0]: https://github.com/erphq/cypher-rs/releases/tag/v0.1.0
