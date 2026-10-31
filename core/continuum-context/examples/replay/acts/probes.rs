//! Her view's own account of each act, from the core log's `delib.view.*` probes: whether her
//! view appended (`advance`, with the characters it appended) or started a new epoch
//! (`turnover`, sized by `sized`). Also the names the core logs beside each persona id, so a
//! resident reads by name. The log is `tracing` output, ANSI colours included.

use std::collections::HashMap;

use crate::acts::clock;

#[derive(Clone, Debug)]
pub struct Probe {
    pub at_ms: u64,
    pub persona_id: String,
    /// `advance`, `turnover`, `sized`.
    pub class: String,
    pub fields: HashMap<String, String>,
}

/// The line without its ANSI escapes (`ESC [ … m`).
fn plain(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The `key=value` fields of a tracing line, span fields (`persona{persona_id=…`) included; a
/// quoted value without its quotes.
fn fields(line: &str) -> HashMap<String, String> {
    line.split_whitespace()
        .filter_map(|word| word.split_once('='))
        .map(|(k, v)| (k.rsplit('{').next().unwrap_or(k).to_owned(), // unwrap_or: rsplit always yields one piece
             v.trim_matches('"').trim_end_matches('}').to_owned()))
        .collect()
}

/// The view probes and persona names in a core log.
#[derive(Default)]
pub struct Probes {
    pub probes: Vec<Probe>,
    pub names: HashMap<String, String>,
}

pub fn parse(text: &str) -> Probes {
    let mut out = Probes::default();
    for line in text.lines() {
        if !line.contains("persona_id") {
            continue;
        }
        let line = plain(line);
        let fields = fields(&line);
        let Some(persona_id) = fields.get("persona_id") else { continue };
        if let Some(name) = fields.get("agent_name") {
            out.names.entry(persona_id.clone()).or_insert_with(|| name.clone());
        }
        let Some(class) = fields.get("probe_class").and_then(|class| class.strip_prefix("delib.view.")) else { continue };
        let Some(at_ms) = line.split_whitespace().next().and_then(clock::parse) else { continue };
        out.probes.push(Probe { at_ms, persona_id: persona_id.clone(), class: class.to_owned(), fields: fields.clone() });
    }
    out.probes.sort_by_key(|probe| probe.at_ms);
    out
}

impl Probes {
    /// Her view's account of the act submitted at `at_ms`, from the probes after `after_ms`:
    /// `turnover <tokens>`, `+<chars>c`, or nothing when no probe was logged.
    pub fn view(&self, persona_id: &str, after_ms: u64, at_ms: u64) -> Option<String> {
        let mine: Vec<&Probe> = self.probes.iter().filter(|p| p.persona_id == persona_id && p.at_ms > after_ms && p.at_ms <= at_ms).collect();
        if mine.iter().any(|p| p.class == "turnover") {
            let sized = mine.iter().rev().find(|p| p.class == "sized").and_then(|p| p.fields.get("view_tokens"));
            return Some(format!("turnover{}", sized.map(|t| format!(" {t}t")).unwrap_or_default())); // unwrap_or_default: a turnover not yet sized shows bare
        }
        let appended: u64 = mine.iter().filter(|p| p.class == "advance").filter_map(|p| p.fields.get("appended_chars")?.parse::<u64>().ok()).sum();
        mine.iter().any(|p| p.class == "advance").then(|| format!("+{appended}c"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = include_str!("fixtures/core-log.txt");

    // what this catches: the ANSI colours left in a field (a persona id that never matches), a
    // turnover reported as an append, or a probe from before her previous act charged to this one.
    #[test]
    fn view_probes_join_her_act_by_persona_and_time() {
        let probes = parse(LOG);
        let id = "0000aaaa-1111-2222-3333-444455556666";
        assert_eq!(probes.names.get(id).map(String::as_str), Some("Testa"));
        let at = |t: &str| clock::parse(t).unwrap_or(0); // unwrap_or: the fixture's stamps parse
        assert_eq!(probes.view(id, at("2026-10-26T22:00:00Z"), at("2026-10-26T22:01:00Z")).as_deref(), Some("+4462c"));
        assert_eq!(probes.view(id, at("2026-10-26T22:01:00Z"), at("2026-10-26T22:05:00Z")).as_deref(), Some("turnover 40173t"));
        assert_eq!(probes.view(id, at("2026-10-26T22:05:00Z"), at("2026-10-26T22:09:00Z")), None);
    }
}
