//! The engine's side of each act, from the llama-server log: the slot that served it, whether
//! her page came in from the sidecar just before, how far her prompt matched the slot (`raw lcp`),
//! and where the engine resumed (`n_past`: the checkpoint it restored, or 0 for a full re-prefill).
//!
//! The log's timestamps restart with each engine process, so they are not joined on: a resume is
//! joined to its capture by `(prompt tokens, resumed position)`, which the capture records as
//! `(usage.inputTokens, timing.cachedTokens)`. Processes are split at `model loaded`; a page-in is
//! the sidecar line on the same slot before the resume.

use std::collections::{HashMap, VecDeque};

#[derive(Clone, Debug, serde::Serialize)]
pub struct Resume {
    /// Which engine process (0 = the first `model loaded` in the log text).
    pub process: usize,
    pub slot: u32,
    pub task: u64,
    /// What the engine did: `restored a checkpoint`, `full re-prefill: <why>`, ...
    pub what: String,
    /// How many tokens of her prompt the slot's cache matched.
    pub lcp: i64,
    pub slot_tokens: u64,
    pub checkpoints: u32,
    /// Where it resumed: prefill starts here.
    pub past: u64,
    pub total: u64,
    /// The persona-id prefix of the sidecar page restored on this slot just before, if any.
    pub paged_in: Option<String>,
}

impl Resume {
    /// Matched tokens the engine could not resume at (matched, then prefilled again).
    pub fn lost(&self) -> u64 {
        u64::try_from(self.lcp).map_or(0, |lcp| lcp.saturating_sub(self.past)) // map_or: a negative lcp (cache off) matched nothing
    }

    /// Tokens of the prompt no cache held.
    pub fn new_tokens(&self) -> u64 {
        u64::try_from(self.lcp).map_or(self.total, |lcp| self.total.saturating_sub(lcp)) // map_or: a negative lcp (cache off) matched nothing
    }

    pub fn full(&self) -> bool {
        self.what.starts_with("full")
    }

    /// The engine re-prefilled more of what it had matched than of what was new: the cache was
    /// there and the resume threw it away.
    pub fn lost_more_than_new(&self) -> bool {
        self.lost() > self.new_tokens()
    }
}

/// `id  N | task M |` → (N, M); task -1 (a slot-level event) as `None`.
fn slot_task(line: &str) -> Option<(u32, Option<u64>)> {
    let rest = &line[line.find("id ")? + 3..];
    let (slot, rest) = rest.split_once('|')?;
    let rest = rest.trim_start().strip_prefix("task ")?;
    let (task, _) = rest.split_once('|')?;
    Some((slot.trim().parse().ok()?, task.trim().parse().ok()))
}

/// `k = v` fields of the resume line's parenthesis.
fn fields(text: &str) -> HashMap<&str, &str> {
    text.split(", ").filter_map(|field| field.split_once(" = ")).map(|(k, v)| (k.trim(), v.trim())).collect()
}

fn resume(line: &str, process: usize, paged: &mut HashMap<u32, String>) -> Option<Resume> {
    let (slot, task) = slot_task(line)?;
    let rest = &line[line.find("| resume: ")? + "| resume: ".len()..];
    let (what, inner) = rest.split_once(" (")?;
    let inner = inner.trim_end().trim_end_matches(')');
    let f = fields(inner);
    let (past, total) = f.get("n_past now")?.split_once(" of ")?;
    Some(Resume {
        process,
        slot,
        task: task?,
        what: what.to_owned(),
        lcp: f.get("raw lcp")?.parse().ok()?,
        slot_tokens: f.get("slot tokens")?.parse().ok()?,
        checkpoints: f.get("checkpoints")?.parse().ok()?,
        past: past.trim().parse().ok()?,
        total: total.trim().parse().ok()?,
        paged_in: paged.remove(&slot),
    })
}

/// The sidecar page a slot restored: `... restored N context checkpoints from the sidecar: .../a-<persona hex>.bin.ckpt`.
fn page_in(line: &str) -> Option<(u32, String)> {
    let path = &line[line.find("context checkpoints from the sidecar: ")?..];
    let (slot, _) = slot_task(line)?;
    let name = path.rsplit('/').next()?;
    let hex = name.strip_prefix("a-").unwrap_or(name); // unwrap_or: a page named without the `a-` prefix is its own name
    Some((slot, hex.chars().take(8).collect()))
}

/// Every resume in `text` (one log, or several concatenated oldest first).
pub fn parse(text: &str) -> Vec<Resume> {
    let mut process = 0usize;
    let mut started = false;
    let mut paged: HashMap<u32, String> = HashMap::new();
    let mut out = Vec::new();
    for line in text.lines() {
        if line.contains("llama_server: model loaded") {
            if started {
                process += 1;
            }
            started = true;
            paged.clear();
        } else if let Some((slot, persona)) = page_in(line) {
            paged.insert(slot, persona);
        } else if let Some(resume) = resume(line, process, &mut paged) {
            out.push(resume);
        }
    }
    out
}

/// Resumes keyed for the join, each key's resumes in log order.
pub struct Index(HashMap<(u64, u64), VecDeque<Resume>>);

impl Index {
    pub fn new(resumes: Vec<Resume>) -> Self {
        let mut map: HashMap<(u64, u64), VecDeque<Resume>> = HashMap::new();
        for resume in resumes {
            map.entry((resume.total, resume.past)).or_default().push_back(resume);
        }
        Index(map)
    }

    /// The resume that served a request of `input_tokens` the capture says resumed at `cached`
    /// (taken: a key that recurs serves the next capture, in order).
    pub fn take(&mut self, input_tokens: u64, cached: u64) -> Option<Resume> {
        self.0.get_mut(&(input_tokens, cached))?.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = include_str!("fixtures/engine-log.txt");

    // what this catches: a page-in attributed to the wrong slot or carried past a relaunch, a
    // resume field misread (lcp vs chosen), or a second process's resumes mixed into the first.
    #[test]
    fn resumes_carry_their_page_in_and_process() {
        let resumes = parse(LOG);
        assert_eq!(resumes.len(), 3);
        let first = &resumes[0];
        assert_eq!((first.process, first.slot, first.lcp, first.past, first.total), (0, 1, 30_000, 11_000, 31_000));
        assert_eq!(first.paged_in.as_deref(), Some("0000aaaa"));
        assert_eq!(first.lost(), 19_000);
        assert!(first.lost_more_than_new());
        // Slot 0's resume came after no page-in.
        assert_eq!(resumes[1].paged_in, None);
        assert!(!resumes[1].lost_more_than_new());
        // The relaunch starts process 1 and forgets a page-in logged before it.
        assert_eq!(resumes[2].process, 1);
        assert!(resumes[2].full());
        assert_eq!(resumes[2].paged_in, None);
    }

    // what this catches: two captures with the same token key both joined to the first resume.
    #[test]
    fn a_recurring_key_is_taken_in_order() {
        let mut index = Index::new(parse(LOG));
        assert_eq!(index.take(31_000, 11_000).map(|r| r.slot), Some(1));
        assert!(index.take(31_000, 11_000).is_none());
    }
}
