//! Skip-condition evaluation — the smallest grammar that covers real recipes,
//! evaluated against [`ExecutionState`]. Deliberately NOT an expression
//! language: a condition that needs more than this is a step that should call
//! a command and bind its answer (commands are the capability surface, not
//! this evaluator).
//!
//! Grammar:
//! - `$path`            — truthy test (null/false/0/""/[]/{} are falsy)
//! - `!$path`           — negated truthy test
//! - `$path == literal` — equality vs a JSON literal (number, "string",
//!   true/false/null)
//! - `$path != literal` — inequality, same literals
//!
//! An unresolved `$path` is FALSY (a skip), never an error: conditions exist
//! to guard steps on optional earlier bindings, and "the binding never
//! happened" is exactly the case they guard.

use super::state::ExecutionState;
use serde_json::Value;

pub fn evaluate(cond: &str, state: &ExecutionState) -> Result<bool, String> {
    let cond = cond.trim();
    for (op, negate) in [("==", false), ("!=", true)] {
        if let Some((lhs, rhs)) = cond.split_once(op) {
            let lhs = resolve(lhs.trim(), state)?;
            let rhs: Value = serde_json::from_str(rhs.trim())
                .map_err(|_| format!("recipe condition `{cond}`: right side must be a JSON literal"))?;
            let eq = lhs == Some(&rhs);
            return Ok(eq != negate);
        }
    }
    if let Some(path) = cond.strip_prefix("!$") {
        return Ok(!truthy(state.lookup(path)));
    }
    if let Some(path) = cond.strip_prefix('$') {
        return Ok(truthy(state.lookup(path)));
    }
    Err(format!(
        "recipe condition `{cond}`: expected `$path`, `!$path`, or `$path ==/!= literal`"
    ))
}

fn resolve<'a>(expr: &str, state: &'a ExecutionState) -> Result<Option<&'a Value>, String> {
    expr.strip_prefix('$')
        .map(|path| state.lookup(path))
        .ok_or_else(|| format!("recipe condition left side `{expr}` must be a `$path`"))
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64() != Some(0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truthiness_comparison_and_unresolved_is_skip() {
        // what this catches: the whole grammar, plus the load-bearing rule
        // that an unresolved path SKIPS (falsy) instead of erroring — that is
        // what lets conditions guard optional bindings.
        let mut s = ExecutionState::default();
        s.bind("decision", serde_json::json!({"shouldRespond": true, "count": 0}));
        assert!(evaluate("$decision.shouldRespond", &s).unwrap());
        assert!(!evaluate("$decision.count", &s).unwrap());
        assert!(evaluate("!$decision.count", &s).unwrap());
        assert!(evaluate("$decision.shouldRespond == true", &s).unwrap());
        assert!(evaluate("$decision.count != 3", &s).unwrap());
        assert!(!evaluate("$never.bound", &s).unwrap());
        assert!(evaluate("not a condition", &s).is_err());
    }
}
