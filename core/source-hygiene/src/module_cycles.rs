//! The crate cuts' precondition, as a ratchet: how many of the core's top-level modules are
//! caught in a dependency cycle.
//!
//! `layer_discipline` counts upward import LINES, and that count can fall for a month while
//! no crate becomes cuttable: a crate is a set of modules that can compile without the rest,
//! and no set can while it sits in a cycle with the rest. On canary `9871a02c0` (2026-10-22)
//! 37 of the core's 50 top-level modules were one strongly connected component, which is why "split into crates" kept stalling. This counts
//! the modules in any cycle (production code, `crate::<module>` references); peeling one off
//! is the step a crate cut needs, and the baseline may only fall.
//!
//! See `docs/architecture/MICROKERNEL-DECOMPOSITION.md` § "Why no cut has landed".

use std::collections::{BTreeMap, BTreeSet};

use super::{CrateRule, SourceFile, Violation};

pub struct ModuleCycles;

/// The top-level module of a scanned file: the first path component under `src/`.
fn module_of(rel: &str) -> &str {
    let first = rel.split('/').next().unwrap_or(rel); // unwrap_or: split always yields a first item
    first.strip_suffix(".rs").unwrap_or(first) // unwrap_or: a directory module has no .rs suffix
}

/// Every top-level module `text`'s code names from the crate root, in the three spellings the
/// tree uses (Cormac on #5266): `crate::<ident>`, a `crate::{…}` group (each top-level path
/// in it, across lines), and a `super::` chain that climbs from `rel`'s depth to the root.
/// Comment lines are skipped: a doc link to a module couples no compilation (`identity`'s only
/// reference to `cognition` was a `//!` link), and a cut must not be refused for one.
/// Undercounting is the failure that matters: a missed spelling lets a cycle pass, and lets a
/// "peel" lower the count by rewriting an import.
fn named_heads(text: &str, rel: &str) -> BTreeSet<String> {
    let code: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut heads = BTreeSet::new();
    for (at, _) in code.match_indices("crate::") {
        heads.extend(heads_after(&code[at + "crate::".len()..]));
    }
    let depth = module_depth(rel);
    let mut at = 0;
    while let Some(found) = code[at..].find("super::") {
        let start = at + found;
        let mut climbs = 0;
        let mut rest = &code[start..];
        while let Some(next) = rest.strip_prefix("super::") {
            climbs += 1;
            rest = next;
        }
        // A `super::` right after another is the tail of a chain already counted at its first
        // one; a chain that stops short of the root stays inside its own top-level module.
        let fresh = !code[..start].ends_with("super::");
        if fresh && climbs >= depth {
            heads.extend(heads_after(rest));
        }
        at = start + "super::".len();
    }
    heads
}

/// How many `super::` reach the crate root from the file at `rel`: its module path's length
/// (`a/b/c.rs` is `a::b::c`, 3; `a/b/mod.rs` is `a::b`, 2; `lib.rs`/`main.rs` are the root).
fn module_depth(rel: &str) -> usize {
    let parts: Vec<&str> = rel.trim_end_matches(".rs").split('/').collect();
    match parts.as_slice() {
        [only] if *only == "lib" || *only == "main" => 0,
        [path @ .., "mod"] => path.len(),
        path => path.len(),
    }
}

/// The module heads a path starting at `rest` names: its first identifier, or, for a
/// `{…}` group, each top-level path's first identifier (`self` and empty items name nothing).
fn heads_after(rest: &str) -> Vec<String> {
    let ident = |s: &str| -> String {
        s.trim_start().chars().take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_').collect()
    };
    let Some(group) = rest.strip_prefix('{') else {
        let head = ident(rest);
        return if head.is_empty() { Vec::new() } else { vec![head] };
    };
    let mut heads = Vec::new();
    let (mut depth, mut item) = (0usize, String::new());
    for c in group.chars() {
        match c {
            '{' => depth += 1,
            '}' if depth == 0 => break,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                heads.push(ident(&item));
                item.clear();
                continue;
            }
            _ => {}
        }
        item.push(c);
    }
    heads.push(ident(&item));
    heads.retain(|head| !head.is_empty() && head != "self");
    heads
}

/// The module graph: each top-level module to the top-level modules its production code names.
pub fn module_graph(files: &[SourceFile]) -> BTreeMap<String, BTreeSet<String>> {
    let modules: BTreeSet<&str> = files.iter().map(|f| module_of(&f.rel)).collect();
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in files {
        let from = module_of(&file.rel);
        let edges = graph.entry(from.to_owned()).or_default();
        for head in named_heads(&file.production, &file.rel) {
            if head != from && modules.contains(head.as_str()) {
                edges.insert(head);
            }
        }
    }
    graph
}

/// The graph's strongly connected components with more than one module (Tarjan), each sorted.
pub fn cycles(graph: &BTreeMap<String, BTreeSet<String>>) -> Vec<Vec<String>> {
    struct Walk<'g> {
        graph: &'g BTreeMap<String, BTreeSet<String>>,
        index: BTreeMap<&'g str, usize>,
        low: BTreeMap<&'g str, usize>,
        stack: Vec<&'g str>,
        on_stack: BTreeSet<&'g str>,
        next: usize,
        found: Vec<Vec<String>>,
    }
    impl<'g> Walk<'g> {
        fn visit(&mut self, v: &'g str) {
            self.index.insert(v, self.next);
            self.low.insert(v, self.next);
            self.next += 1;
            self.stack.push(v);
            self.on_stack.insert(v);
            for w in self.graph.get(v).into_iter().flatten() {
                let w = w.as_str();
                if !self.index.contains_key(w) {
                    self.visit(w);
                    let lw = self.low[w];
                    let lv = self.low[v];
                    self.low.insert(v, lv.min(lw));
                } else if self.on_stack.contains(w) {
                    let iw = self.index[w];
                    let lv = self.low[v];
                    self.low.insert(v, lv.min(iw));
                }
            }
            if self.low[v] == self.index[v] {
                let mut component = Vec::new();
                while let Some(w) = self.stack.pop() {
                    self.on_stack.remove(w);
                    component.push(w.to_owned());
                    if w == v {
                        break;
                    }
                }
                if component.len() > 1 {
                    component.sort();
                    self.found.push(component);
                }
            }
        }
    }
    let mut walk = Walk {
        graph,
        index: BTreeMap::new(),
        low: BTreeMap::new(),
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        next: 0,
        found: Vec::new(),
    };
    for v in graph.keys() {
        if !walk.index.contains_key(v.as_str()) {
            walk.visit(v);
        }
    }
    walk.found.sort_by(|a, b| b.len().cmp(&a.len()));
    walk.found
}

impl CrateRule for ModuleCycles {
    fn name(&self) -> &'static str {
        "module_cycles"
    }

    /// One violation per module caught in a cycle, naming the cycle it is in.
    fn check_crate(&self, files: &[SourceFile], _corpus: &str) -> Vec<Violation> {
        let graph = module_graph(files);
        cycles(&graph)
            .into_iter()
            .flat_map(|component| {
                let size = component.len();
                component.clone().into_iter().map(move |module| Violation {
                    rule: "module_cycles",
                    file: module,
                    line: 0,
                    source: format!("in a cycle of {size} modules: {}", component.join(", ")),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan_crate;

    fn file(rel: &str, production: &str) -> SourceFile {
        SourceFile::for_test(rel, production)
    }

    // what this catches: a cycle read as layers (a crate cut planned through it), or a
    // one-way import read as a cycle (a crate cut refused for nothing).
    #[test]
    fn a_cycle_is_found_and_a_layer_is_not() {
        let files = [
            file("kernel/a.rs", "fn f() {}\n"),
            file("mind.rs", "use crate::kernel::a;\nuse crate::surface::x;\n"),
            // a doc link couples no compilation
            file("kernel/b.rs", "//! see [`crate::surface::x`]\nfn g() {}\n"),
            file("surface/mod.rs", "use crate::mind::y;\nuse crate::kernel::b;\n"),
        ];
        let found = cycles(&module_graph(&files));
        assert_eq!(found, vec![vec!["mind".to_owned(), "surface".to_owned()]]);
        assert_eq!(ModuleCycles.check_crate(&files, "").len(), 2, "one violation per module in a cycle");
    }

    // what this catches (Cormac on #5266): a cycle hidden by how an import is spelled. A
    // grouped `crate::{…}` (across lines) and a `super::` chain that climbs to the root are
    // edges like `crate::x`; a chain that stays inside its own module, and `self` in a group,
    // are not.
    #[test]
    fn every_spelling_of_a_root_path_is_an_edge() {
        let grouped = [
            file("mind.rs", "use crate::{\n    clog_info,\n    surface::{x, y},\n};\n"),
            file("surface/mod.rs", "use crate::mind::z;\n"),
        ];
        assert_eq!(cycles(&module_graph(&grouped)), vec![vec!["mind".to_owned(), "surface".to_owned()]]);
        let climbed = [
            file("mind/a/b.rs", "use super::super::super::surface::x;\nuse super::sibling::y;\n"),
            file("surface/mod.rs", "use crate::mind::a;\n"),
        ];
        assert_eq!(cycles(&module_graph(&climbed)), vec![vec!["mind".to_owned(), "surface".to_owned()]]);
        assert_eq!(heads_after("{self, kernel::a, ,}"), vec!["kernel".to_owned()]);
        assert_eq!(module_depth("mind/a/mod.rs"), 2);
        assert_eq!(module_depth("lib.rs"), 0);
    }

    #[test]
    fn modules_in_cycles_never_increase() {
        let violations = scan_crate(&[&ModuleCycles]);
        let count = violations.len();
        let largest = violations.first().map_or("", |v| v.source.as_str()); // map_or: no cycle, nothing to name
        eprintln!("modules in a cycle: {count}; largest {largest}");
        crate::ratchet::hold("modules_in_cycles", count, || {
            "A new import closes a dependency loop: move the fact or verb down to its owner, or \
             invert it behind a trait the lower module owns."
                .to_owned()
        });
    }
}
