//! The one architectural invariant, as a ratchet: a module imports only its own ring or
//! the rings below it. See `docs/architecture/MICROKERNEL-DECOMPOSITION.md` for the
//! rings, the measurement this baseline came from, and the cut order that lowers it.
//!
//! Why a ratchet and not a rule with zero tolerance: on canary `bc42092af` (2026-10-18)
//! the core carried 256 upward imports across 73 module pairs (`cognition` → `modules` in
//! 27 files, `persona` → `modules` in 20, `runtime` → `sdk_codegen` in 7, ...). A rule that
//! fails on all of them today is a rule nobody runs. A baseline that may only FALL is a
//! rule with teeth on day one, the same shape as `unwrap_justification`.
//!
//! What counts: one violation per (line, imported module) where the file's top-level
//! module sits in a LOWER ring than the module it names with `crate::<mod>::`. Counted over
//! the whole file (tests included): a test that reaches upward still couples the compile
//! unit, and the crate split that this invariant exists for cannot cut through it.
//!
//! Per LINE, not per file: a per-file count rose by one every time a file was split by
//! owner (the same `use crate::modules::x` read as two violations once the tests moved to
//! a sibling file; #5224 read +7 from pure moves). A line is the coupling site itself, so a
//! pure move cannot change the count and only removing a coupling lowers it.

use super::{SourceFile, SourceRule, Violation};

/// Ring 0, the kernel: the machine. Nothing about models, minds, rooms or commands.
const KERNEL: &[&str] = &[
    "time", "identity", "paths", "concurrency", "paging", "logging", "orm",
    "system_resources", "runtime", "governor", "routing", "sdk_codegen", "resources", "gpu",
];
/// Ring 1, the engine: serving. Knows the kernel.
const ENGINE: &[&str] = &[
    "inference", "inference_capability", "model_registry", "capacity", "ai", "live", "media", "sensory",
    "tool_parsing",
];
/// Ring 2, the mind: the citizen. Knows the engine and the kernel.
const MIND: &[&str] = &[
    "cognition", "persona", "genome", "memory", "rag", "perception", "experience", "recipe",
    "context", "vdd",
];
/// Ring 3, the surface: how the outside reaches in. Knows everything below.
const SURFACE: &[&str] = &[
    "commands", "ipc", "airc", "http", "modules", "interface",
    "contracts", "events", "forge", "provisioning", "comms", "ffi", "bin",
];

/// The ring a top-level module lives in, or `None` for a module the table has not placed
/// (a new module is placed by editing the table, never by the ratchet guessing).
pub fn ring_of(module: &str) -> Option<u8> {
    [KERNEL, ENGINE, MIND, SURFACE]
        .iter()
        .position(|ring| ring.contains(&module))
        .map(|r| r as u8)
}

/// The top-level module of a scanned file: the first path component under `src/`.
fn module_of(file: &SourceFile) -> Option<&str> {
    let first = file.rel.split('/').next()?;
    Some(first.strip_suffix(".rs").unwrap_or(first))
}

/// The top-level modules a path after `crate::` names: `cognition::x` names `cognition`;
/// a group `{cognition::x, modules::{a, b}, time}` names each head at depth one, across
/// lines (rustfmt wraps long groups). A head followed by neither `::` nor a group end is a
/// local item, not a module, and is skipped by `ring_of` returning `None`.
pub(crate) fn named_modules(rest: &str) -> Vec<&str> {
    let ident_end = |s: &str| s.find(|c: char| !(c.is_ascii_lowercase() || c == '_')).unwrap_or(s.len());
    let Some(group) = rest.strip_prefix('{') else {
        let end = ident_end(rest);
        return if rest[end..].starts_with("::") { vec![&rest[..end]] } else { Vec::new() };
    };
    let mut heads = Vec::new();
    let mut depth = 1usize;
    let mut at_head = true;
    let mut i = 0;
    while i < group.len() && depth > 0 {
        let tail = &group[i..];
        let c = tail.chars().next().unwrap_or(' '); // unwrap_or: i < len, so a char exists
        if at_head && depth == 1 && c.is_ascii_lowercase() {
            let end = ident_end(tail);
            heads.push(&tail[..end]);
            at_head = false;
            i += end;
            continue;
        }
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 1 => at_head = true,
            _ => {}
        }
        i += c.len_utf8();
    }
    heads
}

pub struct LayerDiscipline;

impl SourceRule for LayerDiscipline {
    fn name(&self) -> &'static str {
        "layer-discipline"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        let Some(own) = module_of(file).and_then(ring_of) else {
            return Vec::new();
        };
        let mut seen: Vec<(usize, &str)> = Vec::new();
        let mut out = Vec::new();
        let raw = file.raw.as_str();
        let mut rest = raw;
        while let Some(at) = rest.find("crate::") {
            let line_no = raw[..raw.len() - rest.len() + at].matches('\n').count();
            rest = &rest[at + "crate::".len()..];
            for dep in named_modules(rest) {
                if seen.contains(&(line_no, dep)) || !ring_of(dep).is_some_and(|r| r > own) {
                    continue;
                }
                // `dep` borrows from `file.raw`: it lives as long as `file`.
                seen.push((line_no, dep));
                out.push(Violation {
                    rule: "layer-discipline",
                    file: file.rel.clone(),
                    line: line_no + 1,
                    source: format!("ring {own} imports ring {} `crate::{dep}::`", ring_of(dep).unwrap_or(own)), // unwrap_or: `is_some_and` above proved it is Some
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    // what this catches: a kernel or engine file reaching UP into a mind or surface module
    // (a fact owned too high, a verb owned too high, or a module in the wrong ring). The
    // one line of code below the ring table is the architecture; everything else follows.
    #[test]
    fn a_lower_ring_importing_a_higher_ring_is_one_violation_per_line_and_module() {
        let f = SourceFile::for_test(
            "runtime/registry.rs",
            "use crate::cognition::input_schema;\nfn f() { crate::modules::x(); crate::modules::y(); crate::time::now(); }\n",
        );
        let v = LayerDiscipline.check(&f);
        let named: Vec<&str> = v.iter().map(|x| x.source.as_str()).collect();
        assert_eq!(v.len(), 2, "cognition and modules once per line, never time: {named:?}");
        // Two coupling sites are two violations, and moving one of them to a sibling file
        // (a split by owner) leaves the total unchanged.
        let two = SourceFile::for_test("runtime/a.rs", "use crate::modules::x;\nfn f() { crate::modules::y(); }\n");
        let one = SourceFile::for_test("runtime/a.rs", "use crate::modules::x;\n");
        let moved = SourceFile::for_test("runtime/a/tests.rs", "fn f() { crate::modules::y(); }\n");
        assert_eq!(LayerDiscipline.check(&two).len(), 2);
        assert_eq!(LayerDiscipline.check(&one).len() + LayerDiscipline.check(&moved).len(), 2);
        assert!(named.iter().any(|s| s.contains("`crate::cognition::`")));
        assert!(named.iter().any(|s| s.contains("`crate::modules::`")));
        // A grouped import names each head, across lines, nested groups included.
        let grouped = SourceFile::for_test(
            "inference/x.rs",
            "use crate::{\n    cognition::a,\n    time::now,\n    modules::{b, c},\n};\n",
        );
        let g: Vec<String> = LayerDiscipline.check(&grouped).into_iter().map(|v| v.source).collect();
        assert_eq!(g.len(), 2, "cognition and modules, never time: {g:?}");
        // Downward and sideways are the architecture, not a violation.
        let ok = SourceFile::for_test("commands/x.rs", "use crate::cognition::a; use crate::runtime::b; use crate::ipc::c;\n");
        assert!(LayerDiscipline.check(&ok).is_empty());
        // A module the table has not placed raises nothing: placing it is an edit to the
        // table, with a reviewer, never a guess by the ratchet.
        let unplaced = SourceFile::for_test("somewhere_new/x.rs", "use crate::ipc::c;\n");
        assert!(LayerDiscipline.check(&unplaced).is_empty());
    }

    // what this catches: the invariant moving the wrong way. Every cut in
    // MICROKERNEL-DECOMPOSITION.md lowers this count; a PR that raises it has put a fact
    // or a verb in a ring above its owner. The ratchet holds the base tree's count.
    #[test]
    fn upward_imports_never_rise() {
        let violations = scan(&[&LayerDiscipline]);
        let count = violations.len();
        let mut pairs = std::collections::BTreeMap::<(String, String), usize>::new();
        for v in &violations {
            let from = v.file.split('/').next().unwrap_or("?").to_string(); // unwrap_or: a rel path always has a first component
            let to = v.source.split('`').nth(1).unwrap_or("?").to_string(); // unwrap_or: the source line is built by `check` with the backticked path
            *pairs.entry((from, to)).or_default() += 1;
        }
        eprintln!("upward imports: {count} across {} module pairs", pairs.len());
        let mut worst: Vec<_> = pairs.into_iter().collect();
        worst.sort_by(|a, b| b.1.cmp(&a.1));
        for ((from, to), n) in worst.iter().take(8) {
            eprintln!("  {from} -> {to}: {n} lines");
        }
        crate::ratchet::hold("upward_imports", count, || {
            "upward imports rose. A lower ring now names a higher ring: move the fact or verb \
             down to its owner, or place the module in its ring."
                .to_string()
        });
    }
}
