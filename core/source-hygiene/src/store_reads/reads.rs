//! Finding a store read in a token stream, and what bounds it.
//!
//! A read is one of three shapes, by ORM:
//! - sea-orm (airc): `.all(db).await` on a `Select`, built as a chain (`Entity::find()...`), in a
//!   binding (`let mut q = ...; q = q.filter(..); q.all(db).await`), or by a helper in the file;
//! - continuum's ORM: a `StorageQuery { .. }` literal (or `StorageQuery::default()`) handed to a
//!   reader, and a `QueryBuilder::new(..)` chain handed to one.
//!
//! A read is BOUNDED when its bound is visible where it is built: `.limit(n)` (or a cursor's
//! `.first(n)` / `.last(n)`) on the chain, in a binding of its root, or in a same-file helper that
//! builds it; `limit: Some(n)` on a literal. An `Option` passed through (`limit: params.limit`) is
//! not a bound: `None` reads the whole table, and the compiler should be the one to refuse it.
//! Anything else bounded by construction says so in a `// bounded: <why>` comment, which the rule
//! reads and a reviewer sees.

use std::collections::BTreeSet;

use crate::lexer::tokens::{matching_close, matching_open, tokenize, Comment, Kind, Token};

/// The methods that bound a sea-orm `Select`. `first`/`last` bound only a cursor (`cursor_by`).
const SEA_BOUNDS: &[&str] = &["limit"];
const CURSOR_BOUNDS: &[&str] = &["first", "last"];

/// What Rust does to rows a store should have filtered, sorted or picked itself.
pub const ROW_FILTERS: &[&str] = &[
    "filter", "retain", "find", "position", "dedup", "dedup_by", "dedup_by_key", "sort", "sort_by", "sort_by_key",
    "sort_unstable", "sort_unstable_by", "sort_unstable_by_key", "max_by", "max_by_key", "min_by", "min_by_key",
];

/// continuum ORM calls that consume a query without materializing its rows.
const NON_READS: &[&str] = &["count", "delete_matching"];

/// continuum ORM calls that return the rows of a query.
const CONTINUUM_READERS: &[&str] = &["query", "query_with_join", "find_matching"];

/// continuum's typed store verbs that take no limit (`continuum-kernel` `orm/store.rs`).
const WHOLE_COLLECTION_VERBS: &[&str] = &["find_all", "find_eq"];

/// Identifiers that end an expression walking backwards: a chain never starts before one.
const KEYWORDS: &[&str] = &[
    "let", "return", "in", "match", "if", "while", "for", "else", "mut", "move", "async", "ref", "break", "loop", "yield", "as",
];

/// How deep a binding is followed (`q` bound from `base`, bound from a chain).
const BINDING_DEPTH: usize = 3;

/// The ORM a read was written against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    SeaOrmAll,
    StorageQueryLiteral,
    QueryBuilderChain,
    /// A continuum reader call (`adapter.query(q).await`): a load, judged only for what Rust does
    /// with its rows afterwards (its query is judged where it is built).
    ContinuumReader,
    /// A typed store verb that takes no limit and loads every matching row
    /// (`store.find_all().await`): each call is an unbounded read, beside the verb's definition.
    WholeCollectionVerb,
}

/// One store read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    pub shape: Shape,
    pub line: usize,
    /// The entity or collection read, as written (`bus_event::Entity`, `T::COLLECTION`), or the
    /// expression the read was handed when the source cannot name it (`span`, `page_rows(..)`).
    pub entity: String,
    /// What bounds it, if anything: `limit`, `cursor first`, `limit: Some(..)`, `bounded: <why>`.
    pub bound: Option<String>,
    /// The typed columns or fields it filters on in the store, for the reach estimate.
    pub filters: BTreeSet<String>,
    /// Rust filtering its rows after the load, at this line, with this method.
    pub filtered_in_rust: Option<(usize, String)>,
}

/// Every store read in `src` (a production half: test items are already blanked).
pub fn reads(src: &str) -> Vec<Read> {
    let tokens = tokenize(src);
    let file = File { code: &tokens.code, comments: &tokens.comments, bodies: fn_bodies(&tokens.code) };
    let mut out = Vec::new();
    for at in 0..file.code.len() {
        if let Some(read) = file.sea_orm_all(at).or_else(|| file.storage_query(at)).or_else(|| file.query_builder(at)).or_else(|| file.continuum_reader(at)) {
            out.push(read);
        }
    }
    out
}

struct Body<'a> {
    name: &'a str,
    open: usize,
    close: usize,
}

struct File<'f, 'a> {
    code: &'f [Token<'a>],
    comments: &'f [Comment<'a>],
    bodies: Vec<Body<'a>>,
}

/// Every `fn` with a body: its name and the token range of its braces.
fn fn_bodies<'a>(code: &[Token<'a>]) -> Vec<Body<'a>> {
    let mut out = Vec::new();
    for at in 0..code.len().saturating_sub(1) {
        if !(code[at].is_ident("fn") && code[at + 1].kind == Kind::Ident) {
            continue;
        }
        let mut j = at + 2;
        while j < code.len() {
            let t = &code[j];
            if t.is('(') || t.is('[') {
                j = matching_close(code, j) + 1;
            } else if t.is(';') {
                break;
            } else if t.is('{') {
                out.push(Body { name: code[at + 1].text, open: j, close: matching_close(code, j) });
                break;
            } else {
                j += 1;
            }
        }
    }
    out
}

impl<'a> File<'_, 'a> {
    fn tok(&self, at: usize) -> Option<&Token<'a>> {
        self.code.get(at)
    }

    /// The innermost function body holding `at`, as (open, close); the whole file outside one.
    fn enclosing(&self, at: usize) -> (usize, usize) {
        self.bodies
            .iter()
            .filter(|b| b.open < at && at <= b.close)
            .max_by_key(|b| b.open)
            .map_or((0, self.code.len().saturating_sub(1)), |b| (b.open, b.close)) // map_or: code outside any fn (a const initializer) is scoped to the file
    }

    /// `.all(<db>).await` at `at` (the `all`), where `<db>` is not a closure: an iterator's
    /// `all` takes a predicate and is never awaited.
    fn sea_orm_all(&self, at: usize) -> Option<Read> {
        if !(self.code[at].is_ident("all") && at > 0 && self.code[at - 1].is('.') && self.tok(at + 1)?.is('(')) {
            return None;
        }
        let close = matching_close(self.code, at + 1);
        let args = &self.code[at + 2..close];
        if args.is_empty() || args.iter().any(|t| t.is('|')) || !(self.tok(close + 1)?.is('.') && self.tok(close + 2)?.is_ident("await")) {
            return None;
        }
        let start = self.chain_start(at - 1);
        let mut ranges = vec![(start, at)];
        self.bindings_of_root(start, at, BINDING_DEPTH, &mut ranges);
        let helpers = self.helper_ranges(&ranges);
        let mut methods = self.methods(&ranges, false);
        methods.extend(self.methods(&helpers, true));
        ranges.extend(helpers);
        let bound = if methods.iter().any(|m| SEA_BOUNDS.contains(&m.as_str())) {
            Some("limit".to_owned())
        } else if methods.contains("cursor_by") && methods.iter().any(|m| CURSOR_BOUNDS.contains(&m.as_str())) {
            Some("cursor first/last".to_owned())
        } else {
            self.justification(start, at)
        };
        Some(Read {
            shape: Shape::SeaOrmAll,
            line: self.code[at].line,
            entity: self.sea_entity(&ranges).unwrap_or_else(|| self.text(start, at.saturating_sub(2))), // unwrap_or_else: a Select the file does not build names the expression it was handed
            bound,
            filters: self.sea_filters(&ranges),
            filtered_in_rust: self.filtered_after(start, close + 2),
        })
    }

    /// A `StorageQuery { .. }` literal or `StorageQuery::default()` at `at` handed to a reader.
    fn storage_query(&self, at: usize) -> Option<Read> {
        if !self.code[at].is_ident("StorageQuery") || !self.in_expression(at) {
            return None;
        }
        let next = self.tok(at + 1)?;
        let (end, limit, entity) = if next.is('{') {
            let close = matching_close(self.code, at + 1);
            (close, self.field(at + 1, close, "limit"), self.field(at + 1, close, "collection"))
        } else if next.is(':') && self.tok(at + 3)?.is_ident("default") && self.tok(at + 4)?.is('(') {
            (matching_close(self.code, at + 4), None, None)
        } else {
            return None;
        };
        let name = self.let_name(at);
        let bound = match limit {
            Some((first, last)) if self.code[first].is_ident("Some") => Some(format!("limit: {}", self.text(first, last))),
            _ => name.and_then(|n| self.assigned_limit(n, end)).or_else(|| self.justification(at, at)),
        };
        if !self.consumed_as_read(at, name, end) {
            return None;
        }
        Some(Read {
            shape: Shape::StorageQueryLiteral,
            line: self.code[at].line,
            entity: entity.map_or_else(|| "(default collection)".to_owned(), |(a, b)| self.text(a, b)), // map_or_else: a literal without a collection reads the default one
            bound,
            filters: BTreeSet::new(),
            filtered_in_rust: None,
        })
    }

    /// A `QueryBuilder::new(..)` chain at `at` handed to a reader.
    fn query_builder(&self, at: usize) -> Option<Read> {
        if !(self.code[at].is_ident("QueryBuilder") && self.tok(at + 1)?.is(':') && self.tok(at + 3)?.is_ident("new") && self.tok(at + 4)?.is('(')) {
            return None;
        }
        let new_close = matching_close(self.code, at + 4);
        let end = self.chain_end(new_close + 1);
        let methods = self.methods(&[(at, end)], false);
        let name = self.let_name(at);
        // A builder returned unbuilt (`fn query(&self) -> QueryBuilder`) is read where its caller
        // builds it, with the caller's limit; this file cannot see that read.
        if (name.is_none() && !methods.contains("build")) || !self.consumed_as_read(at, name, end) {
            return None;
        }
        let mut filters = BTreeSet::new();
        for j in at..end {
            if (self.code[j].is_ident("filter_eq") || self.code[j].is_ident("filter")) && self.tok(j + 2).is_some_and(|t| t.kind == Kind::Literal) {
                filters.insert(self.code[j + 2].text.trim_matches('"').to_owned());
            }
        }
        let bound = if methods.contains("limit") { Some("limit".to_owned()) } else { name.and_then(|n| self.assigned_limit(n, end)).or_else(|| self.justification(at, end)) };
        Some(Read {
            shape: Shape::QueryBuilderChain,
            line: self.code[at].line,
            entity: self.text(at + 5, new_close.saturating_sub(1)),
            bound,
            filters,
            filtered_in_rust: None,
        })
    }

    /// `.query(q).await` (and its siblings): continuum's load, judged for what follows it.
    fn continuum_reader(&self, at: usize) -> Option<Read> {
        let t = &self.code[at];
        let whole = WHOLE_COLLECTION_VERBS.contains(&t.text);
        if !(t.kind == Kind::Ident && (whole || CONTINUUM_READERS.contains(&t.text)) && at > 0 && self.code[at - 1].is('.') && self.tok(at + 1)?.is('(')) {
            return None;
        }
        let close = matching_close(self.code, at + 1);
        if !(self.tok(close + 1)?.is('.') && self.tok(close + 2)?.is_ident("await")) {
            return None;
        }
        let start = self.chain_start(at - 1);
        Some(Read {
            shape: if whole { Shape::WholeCollectionVerb } else { Shape::ContinuumReader },
            line: t.line,
            // The query handed in, or the typed store the verb reads (`self.runs.find_all()`).
            entity: if close > at + 2 { self.text(at + 2, close - 1) } else { self.text(start, at.saturating_sub(2)) },
            bound: if whole { self.justification(start, at) } else { None },
            filters: BTreeSet::new(),
            filtered_in_rust: self.filtered_after(start, close + 2),
        })
    }

    /// The first token of the expression whose method call follows the `.` at `dot`.
    fn chain_start(&self, dot: usize) -> usize {
        let code = self.code;
        let mut i = dot;
        loop {
            if i == 0 {
                return 0;
            }
            let p = &code[i - 1];
            if p.is(')') || p.is(']') {
                match matching_open(code, i - 1) {
                    Some(open) => i = open,
                    None => return i,
                }
            } else if p.is('>') {
                match matching_open(code, i - 1) {
                    Some(open) if open >= 2 && code[open - 1].is(':') && code[open - 2].is(':') => i = open - 2,
                    _ => return i,
                }
            } else if p.is('?') {
                i -= 1;
            } else if p.kind == Kind::Ident && !KEYWORDS.contains(&p.text) {
                i -= 1;
                if i >= 1 && code[i - 1].is('.') {
                    i -= 1;
                } else if i >= 2 && code[i - 1].is(':') && code[i - 2].is(':') {
                    i -= 2;
                } else {
                    return i;
                }
            } else {
                return i;
            }
        }
    }

    /// The token after a method chain starting at `from` (`.a(..).b::<T>(..)?`).
    fn chain_end(&self, mut from: usize) -> usize {
        while let Some(t) = self.tok(from) {
            if t.is('?') {
                from += 1;
            } else if t.is('.') && self.tok(from + 1).is_some_and(|n| n.kind == Kind::Ident) {
                from += 2;
                if self.tok(from).is_some_and(|n| n.is(':')) && self.tok(from + 2).is_some_and(|n| n.is('<')) {
                    from = self.close_angle(from + 2) + 1;
                }
                if self.tok(from).is_some_and(|n| n.is('(')) {
                    from = matching_close(self.code, from) + 1;
                }
            } else {
                break;
            }
        }
        from
    }

    fn close_angle(&self, open: usize) -> usize {
        let mut depth = 0usize;
        for j in open..self.code.len() {
            if self.code[j].is('<') {
                depth += 1;
            } else if self.code[j].is('>') {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
        }
        self.code.len() - 1
    }

    /// The ranges every binding of the chain's root variable assigns it from, followed `depth`
    /// bindings deep: `let [mut] q = ..;`, `q = ..;`, `for q in .. {`.
    fn bindings_of_root(&self, start: usize, use_at: usize, depth: usize, out: &mut Vec<(usize, usize)>) {
        let root = &self.code[start];
        if depth == 0 || root.kind != Kind::Ident || root.text == "self" || root.text == "Self" || !self.tok(start + 1).is_some_and(|t| t.is('.')) {
            return;
        }
        let (open, _) = self.enclosing(use_at);
        for j in open..start {
            if !self.code[j].is_ident(root.text) || self.code[j - 1].is('.') {
                continue;
            }
            let rhs = if self.code[j - 1].is_ident("let") || (self.code[j - 1].is_ident("mut") && self.code[j - 2].is_ident("let")) {
                self.after_eq(j + 1).map(|from| (from, self.statement_end(from, ';')))
            } else if self.tok(j + 1).is_some_and(|t| t.is('=')) && !self.tok(j + 2).is_some_and(|t| t.is('=')) && !self.code[j - 1].is(':') {
                Some((j + 2, self.statement_end(j + 2, ';')))
            } else if self.code[j - 1].is_ident("for") && self.tok(j + 1).is_some_and(|t| t.is_ident("in")) {
                Some((j + 2, self.statement_end(j + 2, '{')))
            } else {
                None
            };
            if let Some((from, to)) = rhs {
                out.push((from, to));
                // The rhs's first token is its own root when it is a chain (`base.filter(..)`).
                if from != start {
                    self.bindings_of_root(from, j, depth - 1, out);
                }
            }
        }
    }

    /// The token after the `=` of a `let` pattern starting at `from` (skipping `: Type`).
    fn after_eq(&self, mut from: usize) -> Option<usize> {
        while let Some(t) = self.tok(from) {
            if t.is(';') || t.is('{') {
                return None;
            }
            if t.is('=') && !self.tok(from + 1).is_some_and(|n| n.is('=') || n.is('>')) {
                return Some(from + 1);
            }
            from += 1;
        }
        None
    }

    /// The index of the first `stop` at group depth zero from `from`.
    fn statement_end(&self, mut from: usize, stop: char) -> usize {
        while let Some(t) = self.tok(from) {
            if t.is(stop) {
                return from;
            }
            if t.is('(') || t.is('[') || t.is('{') {
                from = matching_close(self.code, from);
            } else if t.is('}') || t.is(')') {
                return from;
            }
            from += 1;
        }
        self.code.len() - 1
    }

    /// The bodies of same-file functions the ranges call at their own level: a helper that
    /// builds the query (`for query in page_rows(..)`).
    fn helper_ranges(&self, ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for &(from, to) in ranges {
            for j in self.indices(from, to, false) {
                let t = &self.code[j];
                if t.kind == Kind::Ident && self.tok(j + 1).is_some_and(|n| n.is('(')) {
                    out.extend(self.bodies.iter().filter(|b| b.name == t.text).map(|b| (b.open, b.close)));
                }
            }
        }
        out
    }

    /// The method and path-call names in the ranges: at group depth zero (the chain's own calls,
    /// never an argument's or another match arm's), or at any depth when `deep` (a helper's body
    /// builds the query anywhere in it).
    fn methods(&self, ranges: &[(usize, usize)], deep: bool) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for &(from, to) in ranges {
            for j in self.indices(from.max(1), to, deep) {
                if self.code[j].kind == Kind::Ident && (self.code[j - 1].is('.') || self.code[j - 1].is(':')) && self.tok(j + 1).is_some_and(|t| t.is('(') || t.is(':')) {
                    out.insert(self.code[j].text.to_owned());
                }
            }
        }
        out
    }

    /// The token indices `from..to`, stepping over every group when not `deep`.
    fn indices(&self, from: usize, to: usize, deep: bool) -> Vec<usize> {
        let to = to.min(self.code.len());
        let mut out = Vec::new();
        let mut j = from;
        while j < to {
            out.push(j);
            let t = &self.code[j];
            j = if !deep && (t.is('(') || t.is('[') || t.is('{')) { matching_close(self.code, j) + 1 } else { j + 1 };
        }
        out
    }

    /// `x::Entity` before a `::find*` call, or the entity a `.find_related(X)` names.
    fn sea_entity(&self, ranges: &[(usize, usize)]) -> Option<String> {
        for &(from, to) in ranges {
            for j in from.max(2)..to {
                let t = &self.code[j];
                if t.kind == Kind::Ident && t.text.starts_with("find") && self.code[j - 1].is(':') && self.code[j - 2].is(':') {
                    let mut s = j - 2;
                    while s >= 3 && self.code[s - 1].kind == Kind::Ident && self.code[s - 2].is(':') && self.code[s - 3].is(':') {
                        s -= 3;
                    }
                    if s >= 1 && self.code[s - 1].kind == Kind::Ident {
                        s -= 1;
                    }
                    return Some(self.text(s, j - 3));
                }
                if (t.is_ident("find_related") || t.is_ident("find_also_related") || t.is_ident("find_with_related")) && self.tok(j + 1).is_some_and(|n| n.is('(')) {
                    return Some(self.text(j + 2, matching_close(self.code, j + 1) - 1));
                }
            }
        }
        None
    }

    /// The `Column::X` names a sea-orm read filters on in the store.
    fn sea_filters(&self, ranges: &[(usize, usize)]) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for &(from, to) in ranges {
            for j in from..to.saturating_sub(2) {
                if self.code[j].is_ident("Column") && self.code[j + 1].is(':') && self.code[j + 2].is(':') && self.tok(j + 3).is_some_and(|t| t.kind == Kind::Ident) {
                    out.insert(self.code[j + 3].text.to_owned());
                }
            }
        }
        out
    }

    /// A `// bounded: <why>` comment on the read's statement (from the line after the previous
    /// statement's end through the read's line), its reason non-empty.
    fn justification(&self, start: usize, at: usize) -> Option<String> {
        let mut b = start;
        while b > 0 && !(self.code[b - 1].is(';') || self.code[b - 1].is('{') || self.code[b - 1].is('}')) {
            b -= 1;
        }
        let from = if b == 0 { 1 } else { self.code[b - 1].line + 1 };
        let from = from.min(self.code[start].line);
        let to = self.code[at].line;
        self.comments.iter().filter(|c| c.line >= from && c.line <= to).find_map(|c| {
            let why = c.text.trim().strip_prefix("bounded:")?.trim();
            (!why.is_empty()).then(|| format!("bounded: {why}"))
        })
    }

    /// Whether the token at `at` begins an expression (not a type in a signature or a decl).
    fn in_expression(&self, at: usize) -> bool {
        at > 0 && {
            let p = &self.code[at - 1];
            !(p.is('>') || p.is(':') || p.is('&') || p.is('<') || p.is(',') && self.inside_angle(at) || p.kind == Kind::Ident && ["struct", "impl", "for", "enum", "type", "use", "dyn", "mut"].contains(&p.text))
        }
    }

    fn inside_angle(&self, at: usize) -> bool {
        let mut depth = 0i32;
        for j in (0..at).rev() {
            let t = &self.code[j];
            if t.is('>') {
                depth += 1;
            } else if t.is('<') {
                if depth == 0 {
                    return true;
                }
                depth -= 1;
            } else if t.is(';') || t.is('{') || t.is('(') || t.is('=') {
                return false;
            }
        }
        false
    }

    /// A literal's field `name` (`name: value`): the value's token range.
    fn field(&self, open: usize, close: usize, name: &str) -> Option<(usize, usize)> {
        let mut j = open + 1;
        while j < close {
            let t = &self.code[j];
            if t.is('(') || t.is('[') || t.is('{') {
                j = matching_close(self.code, j) + 1;
                continue;
            }
            if t.is_ident(name) && self.code[j + 1].is(':') && !self.code[j + 2].is(':') {
                let end = self.statement_end(j + 2, ',');
                return Some((j + 2, end.min(close) - 1));
            }
            j += 1;
        }
        None
    }

    /// `NAME` when the expression at `at` is the whole right-hand side of `let [mut] NAME = `.
    /// A type annotation between the name and the `=` is stepped over (`let rows: Vec<T> = ..`).
    fn let_name(&self, at: usize) -> Option<&'a str> {
        if !self.code.get(at.checked_sub(1)?)?.is('=') {
            return None;
        }
        let mut j = at - 1;
        while !self.code[j].is_ident("let") {
            if j == 0 || self.code[j].is(';') || self.code[j].is('{') || self.code[j].is('}') {
                return None;
            }
            j -= 1;
        }
        let name_at = if self.code.get(j + 1)?.is_ident("mut") { j + 2 } else { j + 1 };
        let (name, after) = (self.code.get(name_at)?, self.code.get(name_at + 1)?);
        (name.kind == Kind::Ident && (after.is('=') || after.is(':'))).then_some(name.text)
    }

    /// A later `NAME.limit = Some(..)` (or `= Some` via a builder reassignment `.limit(`).
    fn assigned_limit(&self, name: &str, after: usize) -> Option<String> {
        let (_, close) = self.enclosing(after);
        (after..close.saturating_sub(4)).find_map(|j| {
            (self.code[j].is_ident(name) && self.code[j + 1].is('.') && self.code[j + 2].is_ident("limit") && self.code[j + 3].is('=') && self.code[j + 4].is_ident("Some"))
                .then(|| "limit: Some(..) assigned".to_owned())
        })
    }

    /// Whether a query built at `at..end` reaches something that materializes rows: not only a
    /// `count`/`delete_matching`. Its direct caller decides, or every later use of its binding.
    fn consumed_as_read(&self, at: usize, name: Option<&str>, end: usize) -> bool {
        let callee = |open: usize| open >= 1 && self.code[open].is('(') && self.code[open - 1].kind == Kind::Ident && NON_READS.contains(&self.code[open - 1].text);
        let Some(name) = name else {
            return !(at >= 1 && callee(at - 1));
        };
        let (_, close) = self.enclosing(at);
        let uses: Vec<usize> = (end..close).filter(|&j| self.code[j].is_ident(name) && !self.code[j - 1].is('.')).collect();
        uses.is_empty() || uses.iter().any(|&j| !callee(j - 1) && !(self.code[j - 1].is('&') && j >= 2 && callee(j - 2)))
    }

    /// Rust filtering a load's rows: on the chain after its `.await` (at `await_at`), or on the
    /// variable the load is bound to, later in the same function.
    fn filtered_after(&self, start: usize, await_at: usize) -> Option<(usize, String)> {
        if let Some(hit) = self.filter_in_chain(await_at + 1) {
            return Some(hit);
        }
        let name = self.let_name(start)?;
        let (_, close) = self.enclosing(start);
        let end = self.statement_end(await_at, ';');
        (end..close)
            .filter(|&j| self.code[j].is_ident(name) && !self.code[j - 1].is('.') && self.tok(j + 1).is_some_and(|t| t.is('.')))
            .find_map(|j| self.filter_in_chain(j + 1).or_else(|| self.handed_to_matcher(j)))
    }

    /// The rows at `at` handed whole to a Rust function as an iterator (`page(rows.iter(), ..)`):
    /// the function matches them in memory, the filtering the store's index should have done.
    fn handed_to_matcher(&self, at: usize) -> Option<(usize, String)> {
        let iter = self.tok(at + 2)?;
        if !((iter.is_ident("iter") || iter.is_ident("into_iter")) && self.tok(at + 3)?.is('(') && (self.code[at - 1].is('(') || self.code[at - 1].is(','))) {
            return None;
        }
        let mut open = at - 1;
        let mut depth = 0usize;
        loop {
            let t = &self.code[open];
            if t.is(')') || t.is(']') || t.is('}') {
                depth += 1;
            } else if t.is('(') || t.is('[') || t.is('{') {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            open = open.checked_sub(1)?;
        }
        if !self.code[open].is('(') {
            return None;
        }
        let callee = self.code.get(open.checked_sub(1)?).filter(|t| t.kind == Kind::Ident && !t.text.starts_with(char::is_uppercase))?; // a constructor (`Ok(rows.into_iter()..)`) returns the rows, it matches nothing
        Some((iter.line, format!("handed to {}()", callee.text)))
    }

    /// The first row filter on the method chain starting at `from`.
    fn filter_in_chain(&self, from: usize) -> Option<(usize, String)> {
        let end = self.chain_end(from);
        (from..end).find_map(|j| {
            let t = &self.code[j];
            (t.kind == Kind::Ident && self.code[j - 1].is('.') && ROW_FILTERS.contains(&t.text) && self.tok(j + 1).is_some_and(|n| n.is('('))).then(|| (t.line, format!(".{}()", t.text)))
        })
    }

    /// The source text of tokens `from..=to`, single-spaced where the source had a gap.
    fn text(&self, from: usize, to: usize) -> String {
        let mut out = String::new();
        for j in from..=to.min(self.code.len().saturating_sub(1)) {
            let t = &self.code[j];
            let tight = out.is_empty() || t.kind == Kind::Punct || out.ends_with(|c: char| "(:.&!<[".contains(c));
            if !tight {
                out.push(' ');
            }
            out.push_str(t.text);
        }
        out
    }
}
