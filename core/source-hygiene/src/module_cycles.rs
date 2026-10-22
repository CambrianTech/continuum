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

/// Every `crate::<ident>` head in `text`'s code. Comment lines are skipped: a doc link to a
/// module couples no compilation (`identity`'s only reference to `cognition` was a `//!`
/// link), and a cut must not be refused for one.
fn named_heads(text: &str) -> BTreeSet<&str> {
    let mut heads = BTreeSet::new();
    let code = text.lines().filter(|line| !line.trim_start().starts_with("//"));
    for line in code {
        heads.extend(line_heads(line));
    }
    heads
}

fn line_heads(text: &str) -> BTreeSet<&str> {
    let mut heads = BTreeSet::new();
    for (at, _) in text.match_indices("crate::") {
        let rest = &text[at + "crate::".len()..];
        let end = rest.find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')).unwrap_or(rest.len()); // unwrap_or: an identifier running to the end of the text ends there
        if end > 0 {
            heads.insert(&rest[..end]);
        }
    }
    heads
}

/// The module graph: each top-level module to the top-level modules its production code names.
pub fn module_graph(files: &[SourceFile]) -> BTreeMap<String, BTreeSet<String>> {
    let modules: BTreeSet<&str> = files.iter().map(|f| module_of(&f.rel)).collect();
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in files {
        let from = module_of(&file.rel);
        let edges = graph.entry(from.to_owned()).or_default();
        for head in named_heads(&file.production) {
            if head != from && modules.contains(head) {
                edges.insert(head.to_owned());
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

    #[test]
    fn modules_in_cycles_never_increase() {
        let violations = scan_crate(&[&ModuleCycles]);
        let count = violations.len();
        let largest = violations.first().map_or("", |v| v.source.as_str()); // map_or: no cycle, nothing to name
        eprintln!("modules in a cycle: {count} (baseline {BASELINE_MODULES_IN_CYCLES}); largest {largest}");
        assert!(
            count <= BASELINE_MODULES_IN_CYCLES,
            "modules caught in a dependency cycle rose to {count} (baseline {BASELINE_MODULES_IN_CYCLES}). \
             A new import closes a loop: move the fact or verb down to its owner, or invert it \
             behind a trait the lower module owns. Never raise the baseline."
        );
        crate::assert_baseline_is_current("BASELINE_MODULES_IN_CYCLES", count, BASELINE_MODULES_IN_CYCLES);
    }

    /// Measured on canary 9871a02c0 (2026-10-22): one cycle of 37 of 50 top-level modules
    /// (40 counting doc links, which couple no compilation: `events`, `identity` and
    /// `interface` were held in by comments alone).
    const BASELINE_MODULES_IN_CYCLES: usize = 37;
}
