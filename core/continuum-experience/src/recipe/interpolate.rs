//! Param interpolation — pure substitution of state references into a step's
//! params, before dispatch.
//!
//! Two forms, per RECIPE-EXECUTION-RUNTIME.md:
//! - A string that is EXACTLY `"$name"` or `"$name.path"` is replaced by the
//!   looked-up JSON value, whatever its type — this is how a step passes a
//!   whole array/object (e.g. a roster) onward.
//! - `${name.path}` EMBEDDED in a longer string renders the value into the
//!   string (strings verbatim, other scalars via JSON, containers rejected —
//!   a container mid-sentence is always an authoring bug).
//!
//! An unresolvable reference FAILS the step, loudly, naming the path — the
//! same fail-loud-by-name contract the legacy `{key}` templating had.

use super::state::ExecutionState;
use serde_json::Value;

pub fn interpolate(params: &Value, state: &ExecutionState) -> Result<Value, String> {
    Ok(match params {
        Value::String(s) => interpolate_string(s, state)?,
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| interpolate(v, state))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| interpolate(v, state).map(|v| (k.clone(), v)))
                .collect::<Result<_, _>>()?,
        ),
        other => other.clone(),
    })
}

fn interpolate_string(s: &str, state: &ExecutionState) -> Result<Value, String> {
    // Whole-value form: "$name.path" (no braces, nothing else in the string).
    if let Some(path) = s.strip_prefix('$') {
        if !path.is_empty() && !path.contains('{') && !s.contains(' ') {
            return state
                .lookup(path)
                .cloned()
                .ok_or_else(|| format!("unresolved recipe reference `${path}`"));
        }
    }
    // Embedded form: "text ${name.path} text".
    if !s.contains("${") {
        return Ok(Value::String(s.to_string()));
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after
            .find('}')
            .ok_or_else(|| format!("unterminated `${{` in recipe param `{s}`"))?;
        let path = &after[..end];
        let val = state
            .lookup(path)
            .ok_or_else(|| format!("unresolved recipe reference `${{{path}}}`"))?;
        match val {
            Value::String(v) => out.push_str(v),
            Value::Number(_) | Value::Bool(_) => out.push_str(&val.to_string()),
            _ => {
                return Err(format!(
                    "recipe reference `${{{path}}}` is a container — embed scalars only, \
                     or pass the whole value with `\"$path\"`"
                ))
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(Value::String(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> ExecutionState {
        let mut s = ExecutionState::with_args(serde_json::json!({"model": "ornith", "n": 2}));
        s.bind("roster", serde_json::json!(["Kira", "Joaquin"]));
        s
    }

    #[test]
    fn whole_value_embedded_and_fail_loud_forms() {
        // what this catches: the three interpolation contracts — whole-value
        // passes containers intact, embedded renders scalars into strings,
        // and an unresolved reference names itself instead of passing through.
        let p = serde_json::json!({
            "teammates": "$roster",
            "note": "serve ${args.model} with ${args.n} reviewers",
            "plain": "no refs here"
        });
        let out = interpolate(&p, &state()).unwrap();
        assert_eq!(out["teammates"], serde_json::json!(["Kira", "Joaquin"]));
        assert_eq!(out["note"], "serve ornith with 2 reviewers");
        assert_eq!(out["plain"], "no refs here");

        let bad = serde_json::json!({"x": "$missing.path"});
        let err = interpolate(&bad, &state()).unwrap_err();
        assert!(err.contains("missing.path"), "err names the path: {err}");
    }
}
