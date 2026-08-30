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
