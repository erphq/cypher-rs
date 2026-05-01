# Goals

## North star
Be the de-facto Rust openCypher front-end. Drop-in for any embedded or
server-side graph DB project that doesn't want to carry libcypher-parser.

## v0 success criteria
- Lexer + parser handle: MATCH ✅, OPTIONAL MATCH ✅, WHERE ✅,
  RETURN ✅, ORDER BY ✅, LIMIT ✅, SKIP ✅, list literals ✅, IN ✅,
  CREATE / MERGE / SET / DELETE / UNWIND (pending)
- Parses 1k random TCK queries without panicking (pending)
- AST is documented and stable (✅ for v0.2 surface)
- Semantic analyzer: variable binding ✅, scope check ✅, schema-aware
  label/rel-type validation via opt-in `Schema` trait ✅ (type checks
  deferred to v0.4)
- Logical plan + algebra ✅ (v0.4 base 8 ops + v0.5: Cartesian / Optional)

## v1 success criteria
- openCypher TCK conformance ≥ 95%
- Logical plan output consumed by FFS executor
- Cost-model trait used by ≥1 external project

## Architecture decisions
- Parser: `pest` for v0 simplicity, may switch to `lalrpop` if perf demands
- AST: enum-heavy, no Box-everywhere; arena alloc later if needed
- Errors: `miette` for span-aware diagnostics
- No async - front-end is pure CPU work

## Non-goals
- Physical execution
- Storage layer
- Neo4j-specific dialect extensions (apoc, gds)

## Out of scope (for now)
- Bolt protocol implementation
- Distributed planning
