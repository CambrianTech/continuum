//! Execution state — the append-only binding map a pipeline runs against.
//!
//! Steps write via `outputTo`; later steps read via `$name` / `${name.path}`
//! references in params and conditions. `args` (the caller's invocation
//! params) is pre-bound under `"args"`, so `$args.model` reads what the
//! dispatcher was called with — the same role `--params` templating plays in
//! the legacy benchmark-recipe format, generalized.

use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ExecutionState {
    bindings: HashMap<String, Value>,
}

impl ExecutionState {
    /// Fresh state with the caller's invocation params bound as `args`.
    pub fn with_args(args: Value) -> Self {
        let mut s = Self::default();
        s.bind("args", args);
        s
    }

    /// As [`Self::with_args`], plus caller-supplied bindings a pipeline may read
    /// before any step has run — the ROOM an activity's pipeline runs in
    /// (`$room.id`, `$room.name`, S3) is the first. Seeds bind after `args`, so a
    /// seed named `args` would shadow it; the executor never seeds that name.
    pub fn seeded(args: Value, seed: impl IntoIterator<Item = (String, Value)>) -> Self {
        let mut s = Self::with_args(args);
        for (name, value) in seed {
            s.bind(name, value);
        }
        s
    }

    /// Bind (or rebind — last write wins, probed by the executor) a value.
    pub fn bind(&mut self, name: impl Into<String>, value: Value) {
        self.bindings.insert(name.into(), value);
    }

    /// Resolve a dotted path (`name.field.0.sub`) against the bindings.
    /// Path segments index objects by key and arrays by integer. `None` =
    /// unresolved — callers decide whether that is an error (interpolation:
    /// yes, loudly) or falsy (conditions).
    pub fn lookup(&self, path: &str) -> Option<&Value> {
        let mut parts = path.split('.');
        let root = parts.next()?;
        let mut cur = self.bindings.get(root)?;
        for part in parts {
            cur = match cur {
                Value::Object(map) => map.get(part)?,
                Value::Array(items) => items.get(part.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        Some(cur)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seeded_room_is_readable_before_any_step_runs() {
        // what this catches: S3 — an activity's pipeline runs IN a room, and the
        // first step must be able to name it (`$room.id`) without a prior step
        // having bound it. If seeding regresses, every `work/create` step in an
        // authored activity posts to nowhere.
        let s = ExecutionState::seeded(
            serde_json::json!({"suite": "swe"}),
            [("room".to_string(), serde_json::json!({"id": "r-1", "name": "job-search"}))],
        );
        assert_eq!(s.lookup("room.id").and_then(|v| v.as_str()), Some("r-1"));
        assert_eq!(s.lookup("args.suite").and_then(|v| v.as_str()), Some("swe"));
    }

    #[test]
    fn lookup_walks_objects_and_arrays() {
        // what this catches: the dotted-path resolver regressing on the two
        // container shapes every command result is made of.
        let mut s = ExecutionState::with_args(serde_json::json!({"model": "m1"}));
        s.bind(
            "roster",
            serde_json::json!({"citizens": [{"name": "Kira"}, {"name": "Atlas"}]}),
        );
        assert_eq!(s.lookup("args.model").unwrap(), "m1");
        assert_eq!(s.lookup("roster.citizens.1.name").unwrap(), "Atlas");
        assert!(s.lookup("roster.citizens.9.name").is_none());
        assert!(s.lookup("absent").is_none());
    }
}
