# Contributing to cypher-rs

Thanks for considering a contribution. cypher-rs is a pure-Rust
openCypher front-end (parse, lower, analyze, plan, optimize, cost).
It has no storage, no executor, and no external services to mock.

## Quickstart

```sh
git clone https://github.com/erphq/cypher-rs
cd cypher-rs
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

All five commands must pass before opening a PR. CI runs the same set.

## Project shape

- `src/parser.rs` - pest grammar entry plus CST construction.
- `src/lower.rs` - CST to AST lowering with binding tables.
- `src/sema.rs` - variable / scope / label / rel-type checks.
- `src/plan.rs` - logical plan construction and pretty-print.
- `src/optimize.rs` - tree-rewrite optimizer (predicate pushdown).
- `src/cost.rs` - `CostModel` trait + `CardinalityCostModel` default.
- `tests/` - integration tests, one file per feature area.
- `examples/demo.rs` - end-to-end pipeline.

## Pluggable traits

Two trait extension points let you customize behavior without forking:

- `Schema` - your storage's view of labels, rel-types, properties.
  Implement to opt into label / rel-type validation. The crate ships
  with a no-op default; pass a real implementation to `analyze()` to
  enable schema-aware checks.
- `CostModel` - per-op cardinality and selectivity estimates.
  `CardinalityCostModel` is the default; users with stats supply a
  more accurate implementation for `estimate_cost()`.

Tests for new features should cover both the no-op default and a
`Fake*` implementation when relevant.

## Adding a grammar production

1. Edit `src/grammar.pest`. Use atomic `kw_*` rules with word-boundary
   checks for keywords (see existing patterns; this is how `OR`
   stopped matching the prefix of `ORDER BY`).
2. Update `src/parser.rs` to handle the new rule.
3. Lower it in `src/lower.rs` to AST.
4. Add tests under `tests/grammar_v0_X.rs` covering accepts and
   rejects.

## Adding a plan op or optimization

1. Add the variant to `Plan` in `src/plan.rs`. Keep `Display` aligned
   with the README mockup style (indented tree).
2. Lower it in `src/lower.rs` if it's a new shape.
3. If it's an optimization, add the rewrite to `src/optimize.rs` and
   prove a cost reduction in `tests/cost.rs`.

## Conventions

- No em dashes in code, comments, or docs.
- Commit messages: `vX.Y: <feature>` for milestones, `feat(plan): ...`
  / `fix(parser): ...` / `docs(...)` for incremental work.
- Keep PRs focused. One grammar production, one plan op, or one
  optimization per PR.
- Update `CHANGELOG.md` under `[Unreleased]` for user-visible changes.

## Releasing

Releases are tagged manually; there is no crates.io publish workflow
yet (planned for v0.10).
