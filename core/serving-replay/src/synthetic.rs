//! Synthetic multi-mind interleavings: N minds with growing contexts, switching between
//! acts, deterministic from a seed. They exercise what captures of today's two residents
//! cannot: four or more minds contending for fewer seats, every act append-only, so the
//! available reuse is near 1 and every point below it is the engine's.

use crate::acts::{Act, ActSequence};
use crate::target::{ActRequest, Message, MindId};

#[derive(Debug, Clone)]
pub struct SyntheticSpec {
    pub minds: usize,
    pub acts: usize,
    /// System prompt every mind shares (the grounding and tool doctrine all residents get).
    pub shared_head_chars: usize,
    /// Each mind's own starting context (her history before the replay begins).
    pub base_chars: usize,
    /// The new perception each act appends.
    pub tail_chars: usize,
    /// Her reply to each act, appended before her next one.
    pub reply_chars: usize,
    /// Chance in [0,1] that the next act belongs to a different mind than the last.
    pub switch: f64,
    pub seed: u64,
    pub n_predict: u32,
}

/// xorshift64*: deterministic across platforms, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const WORDS: &[&str] = &[
    "the", "mind", "reads", "a", "file", "and", "opens", "card", "tool", "result", "branch", "commit", "test",
    "passes", "review", "room", "message", "seat", "page", "token", "prefill", "cache", "engine", "budget",
    "Rust", "crate", "module", "trait", "fn", "error", "line", "diff", "merge", "she", "writes", "then",
];

/// `chars` of word text, deterministic from `rng`.
fn prose(rng: &mut Rng, chars: usize) -> String {
    let mut out = String::with_capacity(chars + 16);
    while out.len() < chars {
        if !out.is_empty() {
            out.push(if rng.below(12) == 0 { '\n' } else { ' ' });
        }
        out.push_str(WORDS[rng.below(WORDS.len())]);
    }
    out
}

pub fn build(spec: &SyntheticSpec) -> ActSequence {
    let mut rng = Rng(spec.seed.max(1));
    let head = prose(&mut rng, spec.shared_head_chars);
    let mut histories: Vec<Vec<Message>> = (0..spec.minds)
        .map(|_| vec![Message { role: "user".into(), content: prose(&mut rng, spec.base_chars) }])
        .collect();
    let mut counts = vec![0usize; spec.minds];
    let mut acts = Vec::with_capacity(spec.acts);
    let mut mind = 0usize;
    for i in 0..spec.acts {
        if i > 0 && spec.minds > 1 && rng.unit() < spec.switch {
            mind = (mind + 1 + rng.below(spec.minds - 1)) % spec.minds;
        }
        let history = &mut histories[mind];
        if counts[mind] > 0 {
            history.push(Message { role: "assistant".into(), content: prose(&mut rng, spec.reply_chars) });
            history.push(Message { role: "user".into(), content: prose(&mut rng, spec.tail_chars) });
        }
        acts.push(Act {
            mind: MindId(format!("mind-{mind}")),
            source: format!("synthetic:mind-{mind}:{}", counts[mind]),
            request: ActRequest { system: Some(head.clone()), messages: history.clone(), tools: vec![], n_predict: spec.n_predict },
        });
        counts[mind] += 1;
    }
    let label = format!(
        "synthetic: {} acts of {} minds, head {} + base {} chars, +{} tail +{} reply per act, switch {}, seed {}",
        spec.acts, spec.minds, spec.shared_head_chars, spec.base_chars, spec.tail_chars, spec.reply_chars, spec.switch, spec.seed
    );
    ActSequence { label, acts }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acts::available_reuse;

    // what this catches: a synthetic sequence that does not test what it claims — every
    // mind must act, contexts must only grow (append-only, so available reuse is high on
    // every repeat act), the minds must actually switch, and the same seed must rebuild
    // the same sequence so two engines are compared on identical input.
    #[test]
    fn minds_switch_and_contexts_grow_append_only() {
        let spec = SyntheticSpec { minds: 4, acts: 24, shared_head_chars: 300, base_chars: 2000, tail_chars: 200, reply_chars: 50, switch: 0.6, seed: 7, n_predict: 1 };
        let seq = build(&spec);
        assert_eq!(seq, build(&spec));
        assert_eq!(seq.minds().len(), 4);
        let switches = seq.acts.windows(2).filter(|w| w[0].mind != w[1].mind).count();
        assert!(switches >= 8, "{switches} switches in 24 acts");
        let mut last: std::collections::HashMap<&MindId, &ActRequest> = Default::default();
        for act in &seq.acts {
            if let Some(prev) = last.get(&act.mind) {
                assert!(act.request.messages.len() > prev.messages.len());
                assert!(available_reuse(Some(prev), &act.request) > 0.85);
            }
            last.insert(&act.mind, &act.request);
        }
    }
}
