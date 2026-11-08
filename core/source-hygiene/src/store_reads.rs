//! **A store read starts newest, with a limit from an index, and the store does the filtering.**
//!
//! Joel, 2026-11-03: no blind ORM selects; every reader starts NEWEST with a LIMIT from an index
//! and pages back on demand; "ratchet it". It was never ratcheted, and on 2026-11-07 airc's
//! `bus_events` reads took 2-14 s: "You always dump the whole sql into memory and do a ton of
//! rows, like a madman." A read without a limit grows with the table, so it is fast on a fresh
//! install and slow forever after; rows loaded to be filtered in Rust pay the decode of every
//! row the store's index could have skipped.
//!
//! Two ratchets, both over the token stream (a chain spans lines; see `reads`):
//! - `unbounded_orm_selects`: a read that materializes rows with no visible bound. sea-orm's
//!   `.all(db).await` whose chain, bindings and same-file builders carry no `.limit(` (or a
//!   cursor's `.first`/`.last`); continuum's `StorageQuery` / `QueryBuilder` handed to a reader
//!   without `limit: Some(..)` / `.limit(..)`. A read bounded by construction (a primary-key `IN`
//!   list of known size) says why in a `// bounded: <why>` comment on its statement, which this
//!   rule reads, so the exemption is in the diff a reviewer reads.
//! - `load_then_filter_rows`: a read's rows filtered, sorted, searched or deduplicated in Rust
//!   (`ROW_FILTERS`) on the chain after the load or on the variable it is bound to, or handed whole
//!   to an in-memory matcher as an iterator (`matched::page(rows.iter(), ..)`), in the same
//!   function. The store filters on typed indexed columns; Rust receives the page it asked for.
//!
//! What they cannot see: rows handed on as a `Vec` and filtered elsewhere, and a query's bound
//! set in another file (`crate::event_query::matching(.., limit, ..)` reads as unbounded until
//! its read says `// bounded:`). Both counts may only fall. The rules are portable: `bin/repo_ratchets.rs` runs them against any repository (airc
//! runs them in its CI with its own base).

pub mod reads;

use super::{SourceFile, SourceRule, Violation};
use reads::{Read, Shape};

pub struct UnboundedOrmSelects;
pub struct LoadThenFilterRows;

/// The rules that know nothing of continuum's layout, run against other repositories too.
pub fn portable_rules() -> [&'static dyn SourceRule; 2] {
    [&UnboundedOrmSelects, &LoadThenFilterRows]
}

/// A read as a failure line: what it reads, its bound, what it filters on, and the code.
fn violation(rule: &'static str, file: &SourceFile, read: &Read, line: usize, what: &str) -> Violation {
    let code = file.production.lines().nth(line - 1).map_or("", str::trim); // map_or: a token's line is always inside the text it came from
    let filters = if read.filters.is_empty() { "none".to_owned() } else { read.filters.iter().cloned().collect::<Vec<_>>().join(",") };
    let bound = read.bound.as_deref().unwrap_or("none"); // unwrap_or: an unbounded read prints its bound as absent
    Violation { rule, file: file.repo.clone(), line, source: format!("{what} entity={} bound={bound} store_filters={filters}: {code}", read.entity) }
}

impl SourceRule for UnboundedOrmSelects {
    fn name(&self) -> &'static str {
        "unbounded_orm_selects"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        reads::reads(&file.production)
            .iter()
            .filter(|r| r.shape != Shape::ContinuumReader && r.bound.is_none())
            .map(|r| violation(self.name(), file, r, r.line, "unbounded read"))
            .collect()
    }
}

impl SourceRule for LoadThenFilterRows {
    fn name(&self) -> &'static str {
        "load_then_filter_rows"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        reads::reads(&file.production)
            .iter()
            .filter_map(|r| {
                let (line, method) = r.filtered_in_rust.as_ref()?;
                Some(violation(self.name(), file, r, *line, &format!("rows loaded then {method} in Rust (read at line {})", r.line)))
            })
            .collect()
    }
}

/// The reads exempted by a `// bounded: <why>` comment, for the receipt: an exemption is a
/// claim a reviewer checks, so the driver prints them beside the counts.
pub fn exemptions(file: &SourceFile) -> Vec<Violation> {
    reads::reads(&file.production)
        .iter()
        .filter(|r| r.bound.as_deref().is_some_and(|b| b.starts_with("bounded:")))
        .map(|r| violation("bounded_exemption", file, r, r.line, "exempt"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    fn advice(rule: &str, violations: &[Violation]) -> String {
        format!(
            "{rule}: start the read newest with a limit from an index (`.order_by_desc(..).limit(n)`, a cursor's \
             `.last(n)`, `limit: Some(n)`) and page back on demand; filter in the store on typed indexed columns. \
             A read bounded by construction carries `// bounded: <why>` on its statement.\nFirst {} of {}:\n{}",
            violations.len().min(20),
            violations.len(),
            violations.iter().take(20).map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>().join("\n")
        )
    }

    // what this catches: a new store read with no limit (a read that grows with its table), or
    // one left in a file a change touched. The count may only fall.
    #[test]
    fn unbounded_orm_selects_never_increase() {
        let violations = scan(&[&UnboundedOrmSelects]);
        crate::ratchet::hold_sites("unbounded_orm_selects", &violations, || advice("unbounded_orm_selects", &violations));
    }

    // what this catches: rows loaded to be filtered, sorted or searched in Rust, which decodes
    // every row the store's index could have skipped. The count may only fall.
    #[test]
    fn load_then_filter_rows_never_increase() {
        let violations = scan(&[&LoadThenFilterRows]);
        crate::ratchet::hold_sites("load_then_filter_rows", &violations, || advice("load_then_filter_rows", &violations));
    }

    fn count(rule: &dyn SourceRule, src: &str) -> usize {
        rule.check(&SourceFile::for_test("store.rs", src)).len()
    }

    // what this catches: the sea-orm shapes, across lines and bindings, that make the rule
    // useful rather than noise: a chain with no limit, a bound in a binding or a same-file
    // builder, a cursor's first/last, a justified exemption, and an iterator's `all`.
    #[test]
    fn sea_orm_reads_are_judged_by_their_visible_bound() {
        let unbounded = "async fn f(db: &Db) { let rows = peer::Entity::find()\n .filter(peer::Column::RoomId.eq(r))\n .all(&self.db)\n .await?; }";
        assert_eq!(count(&UnboundedOrmSelects, unbounded), 1);
        let limited = "async fn f() { event::Entity::find().order_by_desc(event::Column::Counter).limit(n).all(db).await }";
        assert_eq!(count(&UnboundedOrmSelects, limited), 0);
        let bound_in_binding = "async fn f() { let mut query = event::Entity::find().limit(limit as u64);\n if x { query = query.filter(c); }\n let rows = query.all(&self.db).await?; }";
        assert_eq!(count(&UnboundedOrmSelects, bound_in_binding), 0);
        let in_helper = "fn page(n: u64) -> Select<E> { E::find().limit(n) }\nasync fn f() { for query in page(n) { query.all(db).await?; } }";
        assert_eq!(count(&UnboundedOrmSelects, in_helper), 0);
        let cursor = "async fn f() { E::find().cursor_by(Column::Id).last(n).all(db).await }";
        assert_eq!(count(&UnboundedOrmSelects, cursor), 0);
        let exempt = "async fn f() {\n // bounded: ids is the caller's page of at most 64 primary keys\n E::find().filter(Column::Id.is_in(ids)).all(db).await }";
        assert_eq!(count(&UnboundedOrmSelects, exempt), 0);
        assert_eq!(exemptions(&SourceFile::for_test("s.rs", exempt)).len(), 1, "an exemption is reported, never silent");
        let empty_reason = "async fn f() {\n // bounded:\n E::find().all(db).await }";
        assert_eq!(count(&UnboundedOrmSelects, empty_reason), 1, "an exemption names its reason");
        assert_eq!(count(&UnboundedOrmSelects, "fn f() { rows.iter().all(|r| r.ok) }"), 0, "an iterator's all is no read");
        assert_eq!(count(&UnboundedOrmSelects, "fn f() { set.all() }"), 0);
        let in_string = "fn f() { let s = \"E::find().all(db).await\"; }";
        assert_eq!(count(&UnboundedOrmSelects, in_string), 0, "a read in a literal is text");
    }

    // what this catches: continuum's ORM shapes: a literal or builder with no limit handed to a
    // reader, an Option passed through as if it were a bound, and the count/delete consumers
    // that load no rows.
    #[test]
    fn continuum_queries_are_judged_by_their_limit() {
        let unbounded = "async fn f() { let query = StorageQuery { collection: T::COLLECTION.into(), ..Default::default() };\n adapter.query(query).await }";
        assert_eq!(count(&UnboundedOrmSelects, unbounded), 1);
        let limited = "async fn f() { adapter.query(StorageQuery { collection: c, limit: Some(page + 1), ..Default::default() }).await }";
        assert_eq!(count(&UnboundedOrmSelects, limited), 0);
        let passthrough = "async fn f() { adapter.query(StorageQuery { collection: c, limit: params.limit, ..Default::default() }).await }";
        assert_eq!(count(&UnboundedOrmSelects, passthrough), 1, "an Option is not a bound: None reads the table");
        let counted = "async fn f() { adapter.count(StorageQuery { collection: c, ..Default::default() }).await;\n let q = StorageQuery { collection: c, ..Default::default() };\n adapter.count(q).await }";
        assert_eq!(count(&UnboundedOrmSelects, counted), 0, "a count loads no rows");
        assert_eq!(count(&UnboundedOrmSelects, "async fn f() { self.find_matching(QueryBuilder::new(T::COLLECTION).build()).await }"), 1);
        assert_eq!(count(&UnboundedOrmSelects, "async fn f() { self.find_matching(QueryBuilder::new(T::COLLECTION).limit(1).build()).await }"), 0);
        assert_eq!(count(&UnboundedOrmSelects, "pub fn build(self) -> StorageQuery { self.query }"), 0, "a type in a signature is no read");
        assert_eq!(count(&UnboundedOrmSelects, "fn query(&self) -> QueryBuilder { QueryBuilder::new(c).filter_eq(\"personaId\", p) }"), 0, "an unbuilt builder is read by its caller");
        assert_eq!(count(&UnboundedOrmSelects, "pub struct StorageQuery { pub limit: Option<usize> }"), 0);
    }

    // what this catches: rows filtered in Rust after a load, on the chain or the bound variable,
    // and the decode-only chains that are not filtering.
    #[test]
    fn rows_filtered_after_a_load_are_counted() {
        let on_chain = "async fn f() { E::find().limit(n).all(db).await?.into_iter().filter(|r| r.kind == k).collect() }";
        assert_eq!(count(&LoadThenFilterRows, on_chain), 1);
        let on_binding = "async fn f() { let mut rows = E::find().all(db).await?;\n rows.retain(|r| r.live);\n rows.sort_by_key(|r| r.at); }";
        assert_eq!(count(&LoadThenFilterRows, on_binding), 1, "one read, counted once at its first filter");
        let decode = "async fn f() { E::find().limit(n).all(db).await?.into_iter().map(from_model).collect() }";
        assert_eq!(count(&LoadThenFilterRows, decode), 0);
        let continuum = "async fn f() { let result = adapter.query(q).await;\n result.data.into_iter().filter(|r| r.ok) }";
        assert_eq!(count(&LoadThenFilterRows, continuum), 1, "the rows behind the result's field are the load's rows");
        let continuum_chain = "async fn f() { let rows = self.find_matching(q).await?;\n rows.iter().find(|r| r.id == id) }";
        assert_eq!(count(&LoadThenFilterRows, continuum_chain), 1);
        let typed_let = "async fn f() { let mut rows: Vec<W> = self.store.find_all().await?.into_iter().map(|(_, r)| r).collect();\n rows.sort_by_key(|r| r.at); }";
        assert_eq!(count(&LoadThenFilterRows, typed_let), 1, "a typed binding is still the load's rows");
        assert_eq!(count(&UnboundedOrmSelects, typed_let), 1, "every find_all call reads its whole collection");
        let matched = "async fn f() { let rows = span.all(db).await?.into_iter().map(from_model).collect::<Result<Vec<_>, E>>()?;\n Ok(matched::page(rows.iter(), channel, limit)) }";
        assert_eq!(count(&LoadThenFilterRows, matched), 1, "rows handed whole to an in-memory matcher");
        let returned = "async fn f() { let rows = E::find().limit(n).all(db).await?;\n Ok(rows.into_iter().map(decode).collect()) }";
        assert_eq!(count(&LoadThenFilterRows, returned), 0);
    }
}
