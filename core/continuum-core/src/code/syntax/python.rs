//! Python validation via `rustpython-parser` — pure Rust, in-process, no interpreter.
//!
//! Replaces three `python3` subprocess spawns per edit (`py_compile`, an AST unbound-name
//! analyzer, a docstring analyzer). All three answers now come from one parse of the same
//! buffer. See the module doc in [`super`] for why the interpreter had to go.

use rustpython_parser::{ast, parse, Mode};
use std::collections::HashSet;

use super::{SyntaxFault, SyntaxValidator};

pub struct PythonValidator;

impl SyntaxValidator for PythonValidator {
    fn language(&self) -> &'static str {
        "Python"
    }

    fn parse_check(&self, source: &str) -> Result<(), SyntaxFault> {
        match parse(source, Mode::Module, "<edit>") {
            Ok(_) => Ok(()),
            Err(err) => Err(SyntaxFault {
                line: line_of_offset(source, err.offset.to_usize()),
                message: err.error.to_string(),
            }),
        }
    }

    /// Names CALLED but never bound anywhere in the module.
    ///
    /// Deliberately conservative — this reports a `NameError` waiting to happen, and a
    /// false positive would refuse a correct edit. So "bound" is read generously: imports
    /// (including `from x import y as z`), def/class names, assignment targets, function
    /// parameters, `for` targets, `with ... as`, and `except ... as` all count. Anything
    /// reached through an attribute (`os.path.join`) is not a bare name and is ignored.
    fn unbound_calls(&self, source: &str) -> Option<Vec<String>> {
        let module = parse_module(source)?;
        let mut bound: HashSet<String> = HashSet::new();
        let mut called: Vec<String> = Vec::new();
        for stmt in &module.body {
            collect_stmt(stmt, &mut bound, &mut called);
        }
        let mut unbound: Vec<String> = called
            .into_iter()
            .filter(|name| !bound.contains(name) && !is_builtin(name))
            .collect();
        unbound.sort();
        unbound.dedup();
        Some(unbound)
    }

    /// Functions that HAD a docstring before the edit and no longer do.
    ///
    /// Only newly-broken ones: a function that never had one is not this edit's business.
    fn displaced_docstrings(&self, before: &str, after: &str) -> Option<Vec<String>> {
        let had = functions_with_docstrings(before)?;
        let has = functions_with_docstrings(after)?;
        let mut lost: Vec<String> = had.difference(&has).cloned().collect();
        lost.sort();
        Some(lost)
    }
}

fn parse_module(source: &str) -> Option<ast::ModModule> {
    match parse(source, Mode::Module, "<edit>").ok()? {
        ast::Mod::Module(m) => Some(m),
        _ => None,
    }
}

/// 1-based line containing `offset`. `None` when the offset is past the end.
fn line_of_offset(source: &str, offset: usize) -> Option<usize> {
    (offset <= source.len()).then(|| source[..offset].bytes().filter(|b| *b == b'\n').count() + 1)
}

/// Names of functions (at any nesting depth) whose first body statement is a string
/// constant — i.e. that currently HAVE a docstring.
fn functions_with_docstrings(source: &str) -> Option<HashSet<String>> {
    let module = parse_module(source)?;
    let mut out = HashSet::new();
    for stmt in &module.body {
        walk_for_docstrings(stmt, &mut out);
    }
    Some(out)
}

fn walk_for_docstrings(stmt: &ast::Stmt, out: &mut HashSet<String>) {
    match stmt {
        ast::Stmt::FunctionDef(f) => {
            if first_stmt_is_string(&f.body) {
                out.insert(f.name.to_string());
            }
            for inner in &f.body {
                walk_for_docstrings(inner, out);
            }
        }
        ast::Stmt::AsyncFunctionDef(f) => {
            if first_stmt_is_string(&f.body) {
                out.insert(f.name.to_string());
            }
            for inner in &f.body {
                walk_for_docstrings(inner, out);
            }
        }
        ast::Stmt::ClassDef(c) => {
            for inner in &c.body {
                walk_for_docstrings(inner, out);
            }
        }
        _ => {}
    }
}

fn first_stmt_is_string(body: &[ast::Stmt]) -> bool {
    matches!(
        body.first(),
        Some(ast::Stmt::Expr(e)) if matches!(e.value.as_ref(), ast::Expr::Constant(c) if c.value.is_str())
    )
}

/// Walk a statement, recording every name it BINDS and every bare name it CALLS.
fn collect_stmt(stmt: &ast::Stmt, bound: &mut HashSet<String>, called: &mut Vec<String>) {
    match stmt {
        ast::Stmt::Import(i) => {
            for alias in &i.names {
                // `import a.b.c` binds `a`; `import a.b as c` binds `c`.
                let name = alias
                    .asname
                    .as_ref()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| {
                        alias
                            .name
                            .split('.')
                            .next()
                            .unwrap_or(alias.name.as_str())
                            .to_string()
                    });
                bound.insert(name);
            }
        }
        ast::Stmt::ImportFrom(i) => {
            for alias in &i.names {
                let name = alias.asname.as_ref().unwrap_or(&alias.name);
                bound.insert(name.to_string());
            }
        }
        ast::Stmt::FunctionDef(f) => {
            bound.insert(f.name.to_string());
            bind_params(&f.args, bound);
            for inner in &f.body {
                collect_stmt(inner, bound, called);
            }
        }
        ast::Stmt::AsyncFunctionDef(f) => {
            bound.insert(f.name.to_string());
            bind_params(&f.args, bound);
            for inner in &f.body {
                collect_stmt(inner, bound, called);
            }
        }
        ast::Stmt::ClassDef(c) => {
            bound.insert(c.name.to_string());
            for inner in &c.body {
                collect_stmt(inner, bound, called);
            }
        }
        ast::Stmt::Assign(a) => {
            for target in &a.targets {
                bind_target(target, bound);
            }
            collect_expr(&a.value, bound, called);
        }
        ast::Stmt::AugAssign(a) => {
            bind_target(&a.target, bound);
            collect_expr(&a.value, bound, called);
        }
        ast::Stmt::AnnAssign(a) => {
            bind_target(&a.target, bound);
            if let Some(v) = &a.value {
                collect_expr(v, bound, called);
            }
        }
        ast::Stmt::For(f) => {
            bind_target(&f.target, bound);
            collect_expr(&f.iter, bound, called);
            for inner in f.body.iter().chain(f.orelse.iter()) {
                collect_stmt(inner, bound, called);
            }
        }
        ast::Stmt::While(w) => {
            collect_expr(&w.test, bound, called);
            for inner in w.body.iter().chain(w.orelse.iter()) {
                collect_stmt(inner, bound, called);
            }
        }
        ast::Stmt::If(i) => {
            collect_expr(&i.test, bound, called);
            for inner in i.body.iter().chain(i.orelse.iter()) {
                collect_stmt(inner, bound, called);
            }
        }
        ast::Stmt::With(w) => {
            for item in &w.items {
                collect_expr(&item.context_expr, bound, called);
                if let Some(v) = &item.optional_vars {
                    bind_target(v, bound);
                }
            }
            for inner in &w.body {
                collect_stmt(inner, bound, called);
            }
        }
        ast::Stmt::Try(t) => {
            for handler in &t.handlers {
                let ast::ExceptHandler::ExceptHandler(h) = handler;
                if let Some(name) = &h.name {
                    bound.insert(name.to_string());
                }
                for inner in &h.body {
                    collect_stmt(inner, bound, called);
                }
            }
            for inner in t
                .body
                .iter()
                .chain(t.orelse.iter())
                .chain(t.finalbody.iter())
            {
                collect_stmt(inner, bound, called);
            }
        }
        ast::Stmt::Return(r) => {
            if let Some(v) = &r.value {
                collect_expr(v, bound, called);
            }
        }
        ast::Stmt::Expr(e) => collect_expr(&e.value, bound, called),
        ast::Stmt::Raise(r) => {
            if let Some(e) = &r.exc {
                collect_expr(e, bound, called);
            }
        }
        _ => {}
    }
}

fn bind_params(args: &ast::Arguments, bound: &mut HashSet<String>) {
    for arg in args
        .args
        .iter()
        .chain(args.posonlyargs.iter())
        .chain(args.kwonlyargs.iter())
    {
        bound.insert(arg.def.arg.to_string());
    }
    if let Some(v) = &args.vararg {
        bound.insert(v.arg.to_string());
    }
    if let Some(k) = &args.kwarg {
        bound.insert(k.arg.to_string());
    }
}

fn bind_target(expr: &ast::Expr, bound: &mut HashSet<String>) {
    match expr {
        ast::Expr::Name(n) => {
            bound.insert(n.id.to_string());
        }
        ast::Expr::Tuple(t) => t.elts.iter().for_each(|e| bind_target(e, bound)),
        ast::Expr::List(l) => l.elts.iter().for_each(|e| bind_target(e, bound)),
        ast::Expr::Starred(s) => bind_target(&s.value, bound),
        _ => {}
    }
}

fn collect_expr(expr: &ast::Expr, bound: &mut HashSet<String>, called: &mut Vec<String>) {
    match expr {
        ast::Expr::Call(c) => {
            // Only BARE names are our business: `os.path.join(...)` is an attribute chain
            // whose root (`os`) is checked as a plain name below, not as a call.
            if let ast::Expr::Name(n) = c.func.as_ref() {
                called.push(n.id.to_string());
            } else {
                collect_expr(&c.func, bound, called);
            }
            for a in &c.args {
                collect_expr(a, bound, called);
            }
            for kw in &c.keywords {
                collect_expr(&kw.value, bound, called);
            }
        }
        ast::Expr::BinOp(b) => {
            collect_expr(&b.left, bound, called);
            collect_expr(&b.right, bound, called);
        }
        ast::Expr::BoolOp(b) => b.values.iter().for_each(|e| collect_expr(e, bound, called)),
        ast::Expr::UnaryOp(u) => collect_expr(&u.operand, bound, called),
        ast::Expr::Compare(c) => {
            collect_expr(&c.left, bound, called);
            c.comparators
                .iter()
                .for_each(|e| collect_expr(e, bound, called));
        }
        ast::Expr::Attribute(a) => collect_expr(&a.value, bound, called),
        ast::Expr::Subscript(s) => {
            collect_expr(&s.value, bound, called);
            collect_expr(&s.slice, bound, called);
        }
        ast::Expr::Tuple(t) => t.elts.iter().for_each(|e| collect_expr(e, bound, called)),
        ast::Expr::List(l) => l.elts.iter().for_each(|e| collect_expr(e, bound, called)),
        ast::Expr::Dict(d) => {
            d.keys
                .iter()
                .flatten()
                .for_each(|e| collect_expr(e, bound, called));
            d.values.iter().for_each(|e| collect_expr(e, bound, called));
        }
        ast::Expr::Await(a) => collect_expr(&a.value, bound, called),
        ast::Expr::JoinedStr(j) => j.values.iter().for_each(|e| collect_expr(e, bound, called)),
        ast::Expr::FormattedValue(f) => collect_expr(&f.value, bound, called),
        _ => {}
    }
}

/// The builtins a module may call without importing. Not exhaustive by design — this list
/// only has to be big enough that we never REFUSE a correct edit; an unknown builtin makes
/// us report a false name, so err toward including.
fn is_builtin(name: &str) -> bool {
    const BUILTINS: &[&str] = &[
        "abs",
        "all",
        "any",
        "bool",
        "bytes",
        "callable",
        "chr",
        "classmethod",
        "dict",
        "dir",
        "divmod",
        "enumerate",
        "eval",
        "exec",
        "filter",
        "float",
        "format",
        "frozenset",
        "getattr",
        "globals",
        "hasattr",
        "hash",
        "hex",
        "id",
        "input",
        "int",
        "isinstance",
        "issubclass",
        "iter",
        "len",
        "list",
        "locals",
        "map",
        "max",
        "min",
        "next",
        "object",
        "oct",
        "open",
        "ord",
        "pow",
        "print",
        "property",
        "range",
        "repr",
        "reversed",
        "round",
        "set",
        "setattr",
        "slice",
        "sorted",
        "staticmethod",
        "str",
        "sum",
        "super",
        "tuple",
        "type",
        "vars",
        "zip",
        // exceptions commonly RAISED by an edit that adds a guard clause
        "Exception",
        "ValueError",
        "TypeError",
        "KeyError",
        "IndexError",
        "RuntimeError",
        "AttributeError",
        "NotImplementedError",
        "StopIteration",
        "AssertionError",
        "OSError",
        "ImportError",
        "ZeroDivisionError",
        "FileNotFoundError",
        "PermissionError",
    ];
    BUILTINS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const V: PythonValidator = PythonValidator;

    // what this catches (THE reason this module exists): the exact shape that broke three
    // SWE-bench runs — a guard clause placed inside an open `def(` parameter list. The
    // closing paren is still there, so delimiters stay BALANCED while the file is
    // unparseable; only a real parser sees it. Previously answered by `python3 -m
    // py_compile`; now answered in-process.
    #[test]
    fn the_flask_shaped_break_is_rejected_and_valid_python_is_not() {
        let valid = "def f(\n    a,\n    b,\n):\n    return a\n";
        assert!(V.parse_check(valid).is_ok());

        let broken = "def f(\n    a,\n    if '.' in a:\n        raise ValueError(\"no dots\")\n    b,\n):\n    return a\n";
        let fault = V
            .parse_check(broken)
            .expect_err("a statement inside a param list is not Python");
        assert!(
            fault.line.is_some(),
            "a refusal must localize the fault so she knows WHERE: {fault}"
        );
    }

    // what this catches: the NameError class the syntax check is blind to — the file
    // parses, then explodes at runtime. Regression for the sympy-21379 edit that inserted
    // a `clear_cache()` call the module never imported.
    #[test]
    fn a_call_to_a_never_imported_name_is_reported() {
        let src = "def f(a):\n    clear_cache()\n    return a\n";
        let unbound = V.unbound_calls(src).expect("python is analyzable");
        assert_eq!(unbound, vec!["clear_cache".to_string()]);
    }

    // what this catches: FALSE POSITIVES, which are worse than misses here — a false name
    // refuses a CORRECT edit. Imports, aliases, params, defs, assignments, comprehension
    // targets and builtins must all read as bound.
    #[test]
    fn legitimately_bound_names_are_never_reported() {
        let src = "\
import os
from json import loads as parse_json

def helper(x):
    return x

class Thing:
    pass

def f(a, *rest, **kw):
    total = sum(rest)
    for item in rest:
        helper(item)
    with open('x') as fh:
        fh.read()
    try:
        parse_json(a)
    except ValueError as e:
        raise RuntimeError(str(e))
    return os.path.join(a), Thing(), total
";
        let unbound = V.unbound_calls(src).expect("analyzable");
        assert!(
            unbound.is_empty(),
            "no correct edit may be refused; got false positives: {unbound:?}"
        );
    }

    // what this catches: the docstring-displacement case — code inserted between `def` and
    // the string demotes it to a bare expression. Only NEWLY broken ones count, so a
    // function that never had a docstring is not reported.
    #[test]
    fn only_newly_displaced_docstrings_are_reported() {
        let before = "def a():\n    \"\"\"Docs.\"\"\"\n    return 1\n\ndef b():\n    return 2\n";
        let after =
            "def a():\n    x = 1\n    \"\"\"Docs.\"\"\"\n    return 1\n\ndef b():\n    return 2\n";
        let lost = V.displaced_docstrings(before, after).expect("analyzable");
        assert_eq!(
            lost,
            vec!["a".to_string()],
            "b never had one — not this edit's business"
        );

        let untouched = V.displaced_docstrings(before, before).expect("analyzable");
        assert!(untouched.is_empty(), "an unchanged file displaces nothing");
    }

    // what this catches: an UNPARSEABLE buffer yields None ("no opinion") from the
    // analyzers rather than an empty Vec. Empty means "I looked and found nothing" and
    // would let a broken file read as clean.
    #[test]
    fn analyzers_have_no_opinion_on_a_file_that_does_not_parse() {
        let broken = "def f(:\n";
        assert!(V.unbound_calls(broken).is_none());
        assert!(V.displaced_docstrings(broken, broken).is_none());
    }
}
