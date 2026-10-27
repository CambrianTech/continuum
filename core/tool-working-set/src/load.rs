//! Reading the records the evaluation runs on: the registry's verbs, each mind's tool calls,
//! and the engine's measured act timings. Each is a file another process wrote.
//!
//! - Registry: the output of `continuum commands/list` (every verb's description and params).
//! - A mind's calls: the output of `continuum cognition/recall-engrams --kind=by_origin
//!   --origin=tool` (her Tool engrams: which verb, when, in which activity, and her act's text).
//! - Act timings: the per-persona prompt captures (`~/.continuum/fixtures/prompt-captures/*.jsonl`),
//!   whose `completed` rows carry the engine's prefill and decode rates.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use crate::cost::ActSample;
use crate::history::{ActivityId, EpochMs, Call};
use crate::vocab::{ToolSchema, Vocabulary};

#[derive(Deserialize)]
struct Registry {
    commands: Vec<RegistryCommand>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryCommand {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    params_schema: Value,
}

/// The vocabulary of a `commands/list` response, each verb priced as the request carries a
/// native tool: name, description and input schema, at the core's schema estimate.
pub fn registry(text: &str) -> Result<Vocabulary, String> {
    let registry: Registry = serde_json::from_str(text).map_err(|e| format!("registry is not a commands/list response: {e}"))?;
    let schemas = registry
        .commands
        .into_iter()
        .map(|command| {
            let mut input_schema = command.params_schema;
            if let Some(object) = input_schema.as_object_mut() {
                // A native spec's input schema carries the type, properties, required and
                // definitions; the registry's JSON-schema root also names its dialect and title.
                object.remove("$schema");
                object.remove("title");
            }
            let spec = serde_json::json!({ "name": command.name, "description": command.description, "input_schema": input_schema });
            let serialized = serde_json::to_string(&spec).map_err(|e| format!("{}: {e}", command.name))?; // the prompt's wire form: a schema is priced as the request carries it
            let tokens = u32::try_from(continuum_base::token_estimate::estimate_tool_schema_tokens(&serialized))
                .map_err(|e| format!("{}: schema size: {e}", command.name))?;
            Ok(ToolSchema { name: command.name, description: command.description, tokens })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Vocabulary::new(schemas)
}

#[derive(Deserialize)]
struct EngramExport {
    engrams: Vec<Engram>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Engram {
    #[serde(default)]
    context_id: Option<String>,
    origin: Value,
    #[serde(default)]
    content: String,
}

/// One mind's calls, oldest first, and what was left out.
#[derive(Debug, Default)]
pub struct MindRecord {
    pub calls: Vec<Call>,
    /// Recorded names that are not verbs of the registry (a path or an argument recorded as
    /// the name of a malformed call, a pseudo-call such as a think-only turn), with counts.
    pub not_verbs: BTreeMap<String, u64>,
    /// Engrams recorded twice under one invocation id.
    pub duplicates: u64,
}

/// A mind's tool calls from a `cognition/recall-engrams` response.
pub fn engrams(text: &str, vocab: &Vocabulary) -> Result<MindRecord, String> {
    let export: EngramExport = serde_json::from_str(text).map_err(|e| format!("not a recall-engrams response: {e}"))?;
    let mut record = MindRecord::default();
    let mut seen: HashSet<Uuid> = HashSet::new();
    for engram in export.engrams {
        if engram.origin.get("kind").and_then(Value::as_str) != Some("Tool") {
            continue;
        }
        let Some(reference) = engram.origin.get("ref") else {
            continue;
        };
        let (Some(name), Some(at)) =
            (reference.get("tool_name").and_then(Value::as_str), reference.get("invoked_at_ms").and_then(Value::as_u64))
        else {
            continue;
        };
        if let Some(invocation) = reference.get("invocation_id").and_then(Value::as_str).and_then(|id| Uuid::parse_str(id).ok()) {
            if !seen.insert(invocation) {
                record.duplicates += 1;
                continue;
            }
        }
        let Some(tool) = vocab.id(name) else {
            *record.not_verbs.entry(name.to_owned()).or_insert(0) += 1;
            continue;
        };
        let activity = engram.context_id.as_deref().and_then(|id| Uuid::parse_str(id).ok()).map(ActivityId);
        record.calls.push(Call { at: EpochMs::from_millis(at), tool, activity, text: engram.content });
    }
    record.calls.sort_by_key(|call| call.at);
    Ok(record)
}

/// The act timings in every prompt capture under `dir`.
pub fn act_samples(dir: &Path) -> Result<Vec<ActSample>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut samples = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        samples.extend(text.lines().filter(|line| line.contains("\"event\":\"completed\"")).filter_map(act_sample));
    }
    Ok(samples)
}

/// One `completed` capture row's timing, when it carries both engine rates.
fn act_sample(line: &str) -> Option<ActSample> {
    let row: Value = serde_json::from_str(line).ok()?;
    let response = row.get("response")?;
    let timing = response.get("timing")?;
    let number = |value: &Value, key: &str| value.get(key).and_then(Value::as_f64);
    let (prefill_ms, decode_ms) = (number(timing, "prefillMs")?, number(timing, "decodeMs")?);
    Some(ActSample {
        at: EpochMs::from_millis(row.get("captured_at_ms").and_then(Value::as_u64)?),
        output_tokens: number(response.get("usage")?, "outputTokens")?,
        prefill_tokens_per_s: number(timing, "prefillTokensPerSecond")?,
        decode_tokens_per_s: number(timing, "decodeTokensPerSecond")?,
        overhead_ms: number(response, "responseTimeMs")? - prefill_ms - decode_ms,
    })
}

/// A list of verb names, one per line; `#` starts a comment.
pub fn names(text: &str) -> Vec<&str> {
    text.lines().map(|line| line.split('#').next().map_or("", str::trim)).filter(|name| !name.is_empty()).collect() // map_or: split always yields a first piece
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a malformed call's recorded name (a file path) entering the history
    // as a verb, a twice-recorded invocation counted twice, or calls left out of time order.
    #[test]
    fn engrams_keep_real_verbs_in_time_order_and_count_what_they_leave_out() {
        let vocab = registry(
            r#"{"commands":[{"name":"code/read","description":"Read a file","paramsSchema":{"$schema":"x","type":"object","properties":{}}},
                            {"name":"work/get","description":"Read a card"}]}"#,
        )
        .expect("registry");
        let read = vocab.id("code/read").expect("listed");
        assert!(vocab.tokens(read) > 8, "priced by its schema");
        let export = r#"{"count":4,"engrams":[
            {"contextId":"5deeb4b2-a8cb-5b4d-9de5-ad014afdaa0f","content":"work/get(card)","origin":{"kind":"Tool","ref":{"tool_name":"work/get","invoked_at_ms":20,"invocation_id":"de3c47b3-e61b-4bb3-bf6d-55ca6fa84ee6"}}},
            {"content":"again","origin":{"kind":"Tool","ref":{"tool_name":"work/get","invoked_at_ms":20,"invocation_id":"de3c47b3-e61b-4bb3-bf6d-55ca6fa84ee6"}}},
            {"content":"read","origin":{"kind":"Tool","ref":{"tool_name":"code/read","invoked_at_ms":10}}},
            {"content":"x","origin":{"kind":"Tool","ref":{"tool_name":"django/db/models/query.py","invoked_at_ms":30}}},
            {"content":"hi","origin":{"kind":"Chat","ref":{"message_id":"m"}}}]}"#;
        let record = engrams(export, &vocab).expect("export");
        let tools: Vec<&str> = record.calls.iter().map(|call| vocab.name(call.tool)).collect();
        assert_eq!(tools, ["code/read", "work/get"]);
        assert_eq!(record.duplicates, 1);
        assert_eq!(record.not_verbs.get("django/db/models/query.py"), Some(&1));
        assert!(record.calls[1].activity.is_some() && record.calls[0].activity.is_none());
        assert_eq!(names("# hands\ncode/read\n\nwork/get  # board\n"), ["code/read", "work/get"]);
    }
}
