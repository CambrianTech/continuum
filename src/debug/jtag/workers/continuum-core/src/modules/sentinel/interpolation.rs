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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::StepResult;
    use serde_json::json;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn make_ctx() -> ExecutionContext {
        ExecutionContext {
            step_results: vec![
                StepResult {
                    step_index: 0,
                    step_type: "shell".to_string(),
                    success: true,
                    duration_ms: 42,
                    output: Some("hello world".to_string()),
                    error: None,
                    exit_code: Some(0),
                    data: json!({ "stdout": "hello world", "exitCode": 0 }),
                },
                StepResult {
                    step_index: 1,
                    step_type: "shell".to_string(),
                    success: false,
                    duration_ms: 10,
                    output: None,
                    error: Some("not found".to_string()),
                    exit_code: Some(127),
                    data: json!({ "stderr": "command not found", "exitCode": 127 }),
                },
            ],
            inputs: {
                let mut m = HashMap::new();
                m.insert("name".to_string(), json!("test-pipeline"));
                m.insert("count".to_string(), json!(5));
                m
            },
            working_dir: PathBuf::from("/tmp/test"),
            named_outputs: {
                let mut m = HashMap::new();
                m.insert("build".to_string(), StepResult {
                    step_index: 0,
                    step_type: "shell".to_string(),
                    success: true,
                    duration_ms: 100,
                    output: Some("build OK".to_string()),
                    error: None,
                    exit_code: Some(0),
                    data: json!({ "artifacts": ["a.o", "b.o"] }),
                });
                m
            },
        }
    }

    // ── Steps path resolution ──

    #[test]
    fn test_steps_output() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.0.output}}", &ctx), "hello world");
    }

    #[test]
    fn test_steps_default_to_output() {
        let ctx = make_ctx();
        // Bare steps.N without field returns output
        assert_eq!(interpolate("{{steps.0}}", &ctx), "hello world");
    }

    #[test]
    fn test_steps_success() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.0.success}}", &ctx), "true");
        assert_eq!(interpolate("{{steps.1.success}}", &ctx), "false");
    }

    #[test]
    fn test_steps_error() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.1.error}}", &ctx), "not found");
        assert_eq!(interpolate("{{steps.0.error}}", &ctx), "");
    }

    #[test]
    fn test_steps_exit_code() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.0.exitCode}}", &ctx), "0");
        assert_eq!(interpolate("{{steps.1.exit_code}}", &ctx), "127");
    }

    #[test]
    fn test_steps_data_field() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.0.data.stdout}}", &ctx), "hello world");
        assert_eq!(interpolate("{{steps.0.data.exitCode}}", &ctx), "0");
    }

    #[test]
    fn test_steps_nested_data() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.1.data.stderr}}", &ctx), "command not found");
    }

    #[test]
    fn test_steps_metadata() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.0.type}}", &ctx), "shell");
        assert_eq!(interpolate("{{steps.0.stepType}}", &ctx), "shell");
        assert_eq!(interpolate("{{steps.0.index}}", &ctx), "0");
        assert_eq!(interpolate("{{steps.0.durationMs}}", &ctx), "42");
    }

    #[test]
    fn test_steps_out_of_bounds() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{steps.99.output}}", &ctx), "");
    }

    // ── Input path resolution ──

    #[test]
    fn test_input_string() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{input.name}}", &ctx), "test-pipeline");
        assert_eq!(interpolate("{{inputs.name}}", &ctx), "test-pipeline");
    }

    #[test]
    fn test_input_number() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{input.count}}", &ctx), "5");
    }

    #[test]
    fn test_input_missing() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{input.nonexistent}}", &ctx), "");
    }

    // ── Named output resolution ──

    #[test]
    fn test_named_output() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{named.build.output}}", &ctx), "build OK");
    }

    #[test]
    fn test_named_success() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{named.build.success}}", &ctx), "true");
    }

    #[test]
    fn test_named_data_field() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{named.build.data.artifacts}}", &ctx), "[\"a.o\",\"b.o\"]");
    }

    #[test]
    fn test_named_missing() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{named.deploy.output}}", &ctx), "");
    }

    // ── Env resolution ──

    #[test]
    fn test_env_var() {
        let ctx = make_ctx();
        std::env::set_var("SENTINEL_TEST_VAR", "sentinel_value");
        assert_eq!(interpolate("{{env.SENTINEL_TEST_VAR}}", &ctx), "sentinel_value");
        std::env::remove_var("SENTINEL_TEST_VAR");
    }

    #[test]
    fn test_env_missing() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{env.NONEXISTENT_VAR_12345}}", &ctx), "");
    }

    // ── Mixed interpolation ──

    #[test]
    fn test_multiple_refs_in_string() {
        let ctx = make_ctx();
        let result = interpolate("Name: {{input.name}}, Output: {{steps.0.output}}", &ctx);
        assert_eq!(result, "Name: test-pipeline, Output: hello world");
    }

    #[test]
    fn test_no_refs_passthrough() {
        let ctx = make_ctx();
        assert_eq!(interpolate("plain text", &ctx), "plain text");
    }

    #[test]
    fn test_unknown_root_preserved() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{unknown.path}}", &ctx), "{{unknown.path}}");
    }

    #[test]
    fn test_whitespace_trimmed() {
        let ctx = make_ctx();
        assert_eq!(interpolate("{{ steps.0.output }}", &ctx), "hello world");
    }

    // ── interpolate_value (JSON) ──

    #[test]
    fn test_interpolate_json_string() {
        let ctx = make_ctx();
        let val = json!("result: {{steps.0.output}}");
        let result = interpolate_value(&val, &ctx);
        assert_eq!(result, json!("result: hello world"));
    }

    #[test]
    fn test_interpolate_json_object() {
        let ctx = make_ctx();
        let val = json!({ "name": "{{input.name}}", "out": "{{steps.0.output}}" });
        let result = interpolate_value(&val, &ctx);
        assert_eq!(result, json!({ "name": "test-pipeline", "out": "hello world" }));
    }

    #[test]
    fn test_interpolate_json_array() {
        let ctx = make_ctx();
        let val = json!(["{{input.name}}", "{{steps.0.output}}"]);
        let result = interpolate_value(&val, &ctx);
        assert_eq!(result, json!(["test-pipeline", "hello world"]));
    }

    #[test]
    fn test_interpolate_preserves_numbers() {
        let ctx = make_ctx();
        let val = json!(42);
        assert_eq!(interpolate_value(&val, &ctx), json!(42));
    }

    #[test]
    fn test_interpolate_preserves_booleans() {
        let ctx = make_ctx();
        let val = json!(true);
        assert_eq!(interpolate_value(&val, &ctx), json!(true));
    }

    // ── evaluate_condition ──

    #[test]
    fn test_condition_true_false() {
        assert!(evaluate_condition("true"));
        assert!(!evaluate_condition("false"));
    }

    #[test]
    fn test_condition_truthy_strings() {
        assert!(evaluate_condition("hello"));
        assert!(evaluate_condition("1"));
        assert!(evaluate_condition("yes"));
    }

    #[test]
    fn test_condition_falsy_values() {
        assert!(!evaluate_condition(""));
        assert!(!evaluate_condition("0"));
        assert!(!evaluate_condition("null"));
        assert!(!evaluate_condition("undefined"));
        assert!(!evaluate_condition("false"));
    }

    #[test]
    fn test_condition_whitespace() {
        assert!(evaluate_condition("  true  "));
        assert!(!evaluate_condition("  false  "));
        assert!(!evaluate_condition("   "));
    }
}
