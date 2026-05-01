# v0.2 TODO

## Semantic analyser

- [ ] Define required methods on the `Schema` trait in `src/sema.rs`:
  - [ ] `fn node_labels(&self) -> &[&str]`
  - [ ] `fn rel_types(&self) -> &[&str]`
  - [ ] `fn property_type(&self, label: &str, prop: &str) -> Option<PropType>`
- [ ] Add `SemaError` variants: `UndeclaredLabel`, `UnknownRelType`, `UnboundVariable`, `AmbiguousReturn`
- [ ] Implement `analyze(query, schema)`:
  - [ ] Validate node labels against the schema
  - [ ] Validate relationship types against the schema
  - [ ] Variable scope analysis (all `WHERE`/`RETURN` vars must be bound in `MATCH`)
  - [ ] Detect duplicate `RETURN` aliases
- [ ] Remove `#[ignore]` from tests in `tests/sema.rs` as each sub-task is completed
- [ ] Add a `sema` feature gate to `Cargo.toml` if the trait requires additional deps
