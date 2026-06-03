//! Tests for the WITH clause (pipeline break between query parts).

use cypher_rs::*;

#[test]
fn parses_simple_with() {
    let q = parse("MATCH (u:User) WITH u RETURN u").unwrap();
    assert_eq!(q.clauses.len(), 3);
    match &q.clauses[1] {
        Clause::With(w) => {
            assert_eq!(w.items.len(), 1);
            assert!(matches!(&w.items[0].expr, Expr::Variable(v) if v == "u"));
        }
        other => panic!("expected WITH, got {other:?}"),
    }
}

#[test]
fn parses_with_alias() {
    let q = parse("MATCH (u:User) WITH u.name AS name RETURN name").unwrap();
    match &q.clauses[1] {
        Clause::With(w) => {
            assert_eq!(w.items.len(), 1);
            assert_eq!(w.items[0].alias.as_deref(), Some("name"));
            assert!(matches!(
                &w.items[0].expr,
                Expr::Property { key, .. } if key == "name"
            ));
        }
        other => panic!("expected WITH, got {other:?}"),
    }
}

#[test]
fn parses_with_multiple_items() {
    let q = parse("MATCH (u:User) WITH u.id AS id, u.name AS name RETURN id, name").unwrap();
    match &q.clauses[1] {
        Clause::With(w) => {
            assert_eq!(w.items.len(), 2);
            assert_eq!(w.items[0].alias.as_deref(), Some("id"));
            assert_eq!(w.items[1].alias.as_deref(), Some("name"));
        }
        other => panic!("expected WITH, got {other:?}"),
    }
}

#[test]
fn parses_with_followed_by_match() {
    let q = parse("MATCH (u:User) WITH u MATCH (m:Movie) RETURN u, m").unwrap();
    assert_eq!(q.clauses.len(), 4);
    assert!(matches!(&q.clauses[0], Clause::Match(_)));
    assert!(matches!(&q.clauses[1], Clause::With(_)));
    assert!(matches!(&q.clauses[2], Clause::Match(_)));
    assert!(matches!(&q.clauses[3], Clause::Return(_)));
}

#[test]
fn parses_with_where_return() {
    let q = parse("MATCH (u:User) WITH u WHERE u.active = true RETURN u").unwrap();
    assert_eq!(q.clauses.len(), 4);
    assert!(matches!(&q.clauses[1], Clause::With(_)));
    assert!(matches!(&q.clauses[2], Clause::Where(_)));
}

#[test]
fn with_alias_is_bound_in_sema() {
    let q = parse("MATCH (u:User) WITH u.name AS name RETURN name").unwrap();
    let report = analyze(&q);
    assert!(
        !report.has_errors(),
        "unexpected errors: {:?}",
        report.issues
    );
    assert!(report.bindings.contains("name"));
}

#[test]
fn with_lowers_to_project_in_plan() {
    let q = parse("MATCH (u:User) WITH u.name AS name RETURN name").unwrap();
    let p = plan(&q).unwrap();
    let rendered = p.to_string();
    // Outer project for RETURN, inner project for WITH.
    assert_eq!(rendered.matches("Project").count(), 2, "plan: {rendered}");
}

#[test]
fn with_lowercase_accepted() {
    let q = parse("MATCH (u) with u RETURN u").unwrap();
    assert!(matches!(&q.clauses[1], Clause::With(_)));
}

#[test]
fn with_is_reserved_keyword() {
    // A variable named `with` should fail to parse.
    assert!(parse("MATCH (with) RETURN with").is_err());
}
