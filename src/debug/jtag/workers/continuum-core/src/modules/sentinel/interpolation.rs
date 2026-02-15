//! Template interpolation for pipeline variable substitution
//!
//! Handles {{variable}} substitution in strings and JSON values.
//! Pure functions — no state needed, just the ExecutionContext.

use regex::Regex;
use serde_json::Value;

use super::types::ExecutionContext;

/// Interpolate {{variable}} references in a template string
pub fn interpolate(template: &str, ctx: &ExecutionContext) -> String {
    let re = Regex::new(r"\{\{([^}]+)\}\}").unwrap();

    re.replace_all(template, |caps: &regex::Captures| {
        let path = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        resolve_path(path, ctx)
    }).to_string()
}

/// Interpolate {{variable}} references in a JSON value recursively
pub fn interpolate_value(value: &Value, ctx: &ExecutionContext) -> Value {
    match value {
        Value::String(s) => Value::String(interpolate(s, ctx)),
        Value::Array(arr) => Value::Array(arr.iter().map(|v| interpolate_value(v, ctx)).collect()),
        Value::Object(obj) => {
            let mut new_obj = serde_json::Map::new();
            for (k, v) in obj {
                new_obj.insert(k.clone(), interpolate_value(v, ctx));
            }
            Value::Object(new_obj)
        }
        _ => value.clone(),
    }
}

/// Evaluate a condition expression (after interpolation)
pub fn evaluate_condition(condition: &str) -> bool {
    let trimmed = condition.trim();

    if trimmed == "true" {
        return true;
    }
    if trimmed == "false" {
        return false;
    }

    // Non-empty string is truthy
    if !trimmed.is_empty() && trimmed != "0" && trimmed != "null" && trimmed != "undefined" {
        return true;
    }

    false
}

/// Resolve a variable path like "steps.0.output" or "input.name"
fn resolve_path(path: &str, ctx: &ExecutionContext) -> String {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return format!("{{{{{path}}}}}");
    }

    match parts[0] {
        "steps" => {
            if parts.len() < 2 {
                return "".to_string();
            }
            let index: usize = parts[1].parse().unwrap_or(usize::MAX);
            if index >= ctx.step_results.len() {
                return "".to_string();
            }
            let result = &ctx.step_results[index];

            if parts.len() == 2 {
                return result.output.clone().unwrap_or_default();
            }

            match parts[2] {
                "output" => result.output.clone().unwrap_or_default(),
                "success" => result.success.to_string(),
                "error" => result.error.clone().unwrap_or_default(),
                "exitCode" | "exit_code" => result.exit_code.map(|c| c.to_string()).unwrap_or_default(),
                "data" => {
                    if parts.len() > 3 {
                        let mut current = &result.data;
                        for part in &parts[3..] {
                            current = current.get(*part).unwrap_or(&Value::Null);
                        }
                        match current {
                            Value::String(s) => s.clone(),
                            _ => current.to_string(),
                        }
                    } else {
                        result.data.to_string()
                    }
                }
                _ => "".to_string(),
            }
        }
        "input" | "inputs" => {
            if parts.len() < 2 {
                return "".to_string();
            }
            ctx.inputs.get(parts[1])
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    _ => v.to_string(),
                })
                .unwrap_or_default()
        }
        "env" => {
            if parts.len() < 2 {
                return "".to_string();
            }
            std::env::var(parts[1]).unwrap_or_default()
        }
        _ => format!("{{{{{path}}}}}"),
    }
}
