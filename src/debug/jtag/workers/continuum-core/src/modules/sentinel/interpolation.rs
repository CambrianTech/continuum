//! Template interpolation for pipeline variable substitution
//!
//! Handles {{variable}} substitution in strings and JSON values.
//! Pure functions — no state needed, just the ExecutionContext.
//!
//! Supported paths:
//! - `{{steps.N.output}}` — output of step N
//! - `{{steps.N.data.field}}` — nested data from step N
//! - `{{input.name}}` / `{{inputs.name}}` — pipeline inputs
//! - `{{named.label.output}}` — named step output
//! - `{{env.VAR}}` — environment variable

use regex::Regex;
use serde_json::Value;

use super::types::ExecutionContext;

/// Interpolate {{variable}} references in a template string
pub fn interpolate(template: &str, ctx: &ExecutionContext) -> String {
    let re = Regex::new(r"\{\{([^}]+)\}\}").unwrap();

    re.replace_all(template, |caps: &regex::Captures| {
        let path = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        resolve_path(path, ctx)
    }).to_string()
}

/// Interpolate {{variable}} references in a JSON value recursively
pub fn interpolate_value(value: &Value, ctx: &ExecutionContext) -> Value {
    match value {
        Value::String(s) => {
            let interpolated = interpolate(s, ctx);
            // Try to parse as JSON if the entire string was a single interpolation
            // This preserves types: {{steps.0.data}} returns object, not stringified JSON
            if s.starts_with("{{") && s.ends_with("}}") && s.matches("{{").count() == 1 {
                if let Ok(parsed) = serde_json::from_str::<Value>(&interpolated) {
                    if !parsed.is_string() {
                        return parsed;
                    }
                }
            }
            Value::String(interpolated)
        }
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

/// Resolve a variable path like "steps.0.output", "input.name", or "named.build.output"
fn resolve_path(path: &str, ctx: &ExecutionContext) -> String {
    let parts: Vec<&str> = path.split('.').collect();
    if parts.is_empty() {
        return format!("{{{{{path}}}}}");
    }

    match parts[0] {
        "steps" => resolve_steps_path(&parts[1..], ctx),
        "input" | "inputs" => resolve_input_path(&parts[1..], ctx),
        "named" => resolve_named_path(&parts[1..], ctx),
        "env" => {
            if parts.len() < 2 {
                return "".to_string();
            }
            std::env::var(parts[1]).unwrap_or_default()
        }
        _ => format!("{{{{{path}}}}}"),
    }
}

/// Resolve steps.N.field paths
fn resolve_steps_path(parts: &[&str], ctx: &ExecutionContext) -> String {
    if parts.is_empty() {
        return "".to_string();
    }

    let index: usize = parts[0].parse().unwrap_or(usize::MAX);
    if index >= ctx.step_results.len() {
        return "".to_string();
    }

    let result = &ctx.step_results[index];
    resolve_step_result_field(result, &parts[1..])
}

/// Resolve named.label.field paths
fn resolve_named_path(parts: &[&str], ctx: &ExecutionContext) -> String {
    if parts.is_empty() {
        return "".to_string();
    }

    let label = parts[0];
    match ctx.named_outputs.get(label) {
        Some(result) => resolve_step_result_field(result, &parts[1..]),
        None => "".to_string(),
    }
}

/// Resolve input.field paths
fn resolve_input_path(parts: &[&str], ctx: &ExecutionContext) -> String {
    if parts.is_empty() {
        return "".to_string();
    }
    ctx.inputs.get(parts[0])
        .map(|v| match v {
            Value::String(s) => s.clone(),
            _ => v.to_string(),
        })
        .unwrap_or_default()
}

/// Extract a field from a StepResult given the remaining path parts
fn resolve_step_result_field(result: &super::types::StepResult, parts: &[&str]) -> String {
    if parts.is_empty() {
        return result.output.clone().unwrap_or_default();
    }

    match parts[0] {
        "output" => result.output.clone().unwrap_or_default(),
        "success" => result.success.to_string(),
        "error" => result.error.clone().unwrap_or_default(),
        "exitCode" | "exit_code" => result.exit_code.map(|c| c.to_string()).unwrap_or_default(),
        "data" => {
            if parts.len() > 1 {
                let mut current = &result.data;
                for part in &parts[1..] {
                    current = current.get(*part).unwrap_or(&Value::Null);
                }
                match current {
                    Value::String(s) => s.clone(),
                    Value::Null => "".to_string(),
                    _ => current.to_string(),
                }
            } else {
                result.data.to_string()
            }
        }
        "type" | "stepType" => result.step_type.clone(),
        "index" | "stepIndex" => result.step_index.to_string(),
        "durationMs" | "duration_ms" => result.duration_ms.to_string(),
        _ => "".to_string(),
    }
}
