//! The awareness strip: every activity she is in, one line each, loudest first.
//!
//! EVENT-MIND.md §6. Awareness is total, attention is hers: the strip is folded
//! from every activity's delta and salience and published as a `watch` snapshot;
//! her turn renders it at the depth her dial sets (a few lines when deep, all of
//! it when broad), beside the depth of the activity she is working in. The load
//! (how many activities are live, how much is unread, what share of her context
//! the strip would take) is shown to her, never used to cap her.
//!
//! Pure fold + a `RagSource` renderer. The region that feeds it is
//! `perception_region`; nothing here reads a room.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::attention::{AttentionDial, Continuation};
use super::salience::{Salience, SalienceLevel};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityLine {
    pub activity: Uuid,
    pub name: String,
    pub unread: u32,
    pub salience: Salience,
    pub last_activity_ms: u64,
    /// Her surprise here, as a number with its counts (the verdict surprise, step 1).
    pub surprise: super::perception_region::SurpriseTally,
    /// Peers whose work or question waits on her in this activity.
    pub waiting_on_me: Vec<Uuid>,
}

/// What spreading thin costs, as numbers she can read.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Load {
    pub live_activities: u32,
    pub unread_total: u32,
    /// Share of her turn's token budget the strip at the current depth would take.
    pub context_share: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AwarenessSnapshot {
    /// Loudest first, then most recent.
    pub lines: Vec<ActivityLine>,
    pub continuation: Option<Continuation>,
    pub dial: AttentionDial,
    pub load: Load,
    pub at_ms: u64,
}

/// Her verdict surprise across every activity on the strip, folded by COUNTS (never an
/// average of ratios: an activity with one judged expectation must not weigh as much as
/// one with twenty). `None` until the room has judged at least one of her stated
/// expectations in the window: not yet measured, which is not low.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerdictSurprise {
    /// contradicted / judged, in [0, 1].
    pub s: f32,
    pub contradicted: u32,
    pub judged: u32,
}

impl AwarenessSnapshot {
    pub fn verdict_surprise(&self) -> Option<VerdictSurprise> {
        let (contradicted, judged) = self.lines.iter().fold((0u32, 0u32), |(c, n), l| {
            (c + l.surprise.contradicted, n + l.surprise.confirmed + l.surprise.contradicted)
        });
        (judged > 0).then(|| VerdictSurprise { s: contradicted as f32 / judged as f32, contradicted, judged })
    }
}

/// Fold the lines into a snapshot. Ordering: salience level desc, then recency
/// desc; `load.context_share` is the MEASURED cost of the lines she would see at
/// the dial's depth (the rendered text, at the crate's chars-per-token estimate)
/// against `turn_budget_tokens`; never a constant, so it scales with the served
/// window like every other budget in cognition.
pub fn fold(
    mut lines: Vec<ActivityLine>,
    continuation: Option<Continuation>,
    dial: AttentionDial,
    turn_budget_tokens: u32,
    at_ms: u64,
) -> AwarenessSnapshot {
    lines.sort_by(|a, b| {
        b.salience
            .level
            .cmp(&a.salience.level)
            .then(b.last_activity_ms.cmp(&a.last_activity_ms))
    });
    let mut snapshot = AwarenessSnapshot {
        lines,
        continuation,
        dial,
        load: Load { live_activities: 0, unread_total: 0, context_share: 0.0 },
        at_ms,
    };
    let rendered_chars: usize = snapshot.render_lines().iter().map(|l| l.chars().count()).sum();
    snapshot.load = Load {
        live_activities: snapshot.lines.len() as u32,
        unread_total: snapshot.lines.iter().map(|l| l.unread).sum(),
        context_share: if turn_budget_tokens == 0 {
            0.0
        } else {
            (rendered_chars / crate::cognition::deliberation_budget::GUARD_CHARS_PER_TOKEN) as f32 / turn_budget_tokens as f32
        },
    };
    snapshot
}

impl AwarenessSnapshot {
    /// The loudest activity, if any is above `Quiet`: what would wake her.
    pub fn loudest(&self) -> Option<&ActivityLine> {
        self.lines.first().filter(|l| l.salience.level > SalienceLevel::Quiet)
    }

    /// The lines she sees at her dial's depth.
    pub fn visible(&self) -> &[ActivityLine] {
        let n = self.lines.len().min(self.dial.strip_lines());
        &self.lines[..n]
    }

    /// One line of text per visible activity, for the renderer. Her own
    /// continuation leads when present, so the task she chose reads first.
    pub fn render_lines(&self) -> Vec<String> {
        self.render_with(|_| true)
    }

    /// The lines as one turn of `persona` in `room` sees them. A continuation in her mind
    /// room is hers (PRIVACY-OF-THOUGHT.md §4): outside her mind room the strip says only
    /// that private work is pending, never its note, so a private continuation never
    /// reaches a public turn's prompt or capture.
    pub fn render_lines_in(&self, persona: Uuid, room: Option<Uuid>) -> Vec<String> {
        self.render_with(|c| {
            !crate::persona::mind_room::is_private_room(persona, c.activity) || room == Some(c.activity)
        })
    }

    fn render_with(&self, shows_note: impl Fn(&Continuation) -> bool) -> Vec<String> {
        let mut out = Vec::with_capacity(self.visible().len() + 1);
        if let Some(c) = &self.continuation {
            out.push(if shows_note(c) {
                format!("[working] {} — {}", short8(c.activity), c.note)
            } else {
                "[working] private work pending in your mind room".to_string()
            });
        }
        for l in self.visible() {
            let mut line = format!("{} · {} unread · {:?}", l.name, l.unread, l.salience.level);
            if !l.waiting_on_me.is_empty() {
                line.push_str(&format!(" · {} waiting on you", l.waiting_on_me.len()));
            }
            if let Some(r) = l.salience.reasons.first() {
                line.push_str(&format!(" · {}", reason_word(r)));
            }
            // The number she asked for (Kimi §12): shown whenever she has stated an
            // expectation here and the room answered it, with its counts.
            if let Some(s) = l.surprise.s() {
                line.push_str(&format!(
                    " · surprise {s:.2} ({} of {} expectations contradicted)",
                    l.surprise.contradicted,
                    l.surprise.confirmed + l.surprise.contradicted
                ));
            }
            out.push(line);
        }
        out
    }
}

fn short8(id: Uuid) -> String {
    id.to_string()[..8].to_string()
}

fn reason_word(r: &super::salience::SalienceReason) -> &'static str {
    use super::salience::SalienceReason::*;
    match r {
        MentionedMe { .. } => "mentioned you",
        HumanSpoke { .. } => "a human spoke",
        VerdictOnMyWork { .. } => "verdict on your work",
        TeammateWaitingOnMe { .. } => "a teammate waits on you",
        MyCardMoved { .. } => "your card moved",
        Blocker { .. } => "BLOCKER on your card",
        Surprise { .. } => "NOT what you expected",
        Silent { .. } => "no reply by the time you expected",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::salience::SalienceReason;

    fn line(name: &str, level: SalienceLevel, unread: u32, at: u64) -> ActivityLine {
        ActivityLine {
            activity: Uuid::new_v4(),
            name: name.into(),
            unread,
            salience: Salience { level, reasons: vec![] },
            last_activity_ms: at,
            surprise: Default::default(),
            waiting_on_me: vec![],
        }
    }

    // what this catches (GENE-REUSE-FORK-MINT §3, S(C) = the verdict surprise until the
    // model surprise exists): the fold is by COUNTS across activities, never a mean of
    // ratios (one judged expectation must not weigh as twenty), and it is None until the
    // room judged at least one of her expectations: not measured is not low.
    #[test]
    fn verdict_surprise_folds_by_counts_and_is_none_until_judged() {
        use crate::persona::perception_region::SurpriseTally;
        let tally = |confirmed, contradicted| SurpriseTally { confirmed, contradicted, since_ms: 1 };
        let mut a = line("a", SalienceLevel::Quiet, 0, 1);
        let mut b = line("b", SalienceLevel::Quiet, 0, 1);
        assert_eq!(fold(vec![a.clone(), b.clone()], None, AttentionDial::broad(), 1_000, 1).verdict_surprise(), None);
        a.surprise = tally(1, 0); // 0 of 1
        b.surprise = tally(0, 19); // 19 of 19
        let v = fold(vec![a, b], None, AttentionDial::broad(), 1_000, 1).verdict_surprise().expect("judged");
        assert_eq!((v.contradicted, v.judged), (19, 20));
        assert!((v.s - 0.95).abs() < 1e-6, "by counts, 19/20, not the mean of 0 and 1: {v:?}");
    }

    // what this catches: the strip's order and the dial's depth. Loudest first, then
    // most recent; Deep shows three lines and Broad shows all; the load reports
    // every live activity and all unread whatever the depth (awareness is total even
    // when attention is narrow), and context_share follows what is shown.
    #[test]
    fn loudest_first_then_recent_depth_cuts_the_view_not_the_load() {
        let lines = vec![
            line("quiet-old", SalienceLevel::Quiet, 0, 1),
            line("notable-new", SalienceLevel::Notable, 2, 9),
            line("notable-old", SalienceLevel::Notable, 1, 5),
            line("addressed", SalienceLevel::Addressed, 4, 3),
            line("urgent", SalienceLevel::Urgent, 1, 2),
        ];
        let deep = fold(lines.clone(), None, AttentionDial::deep(), 1_000, 10);
        let names: Vec<_> = deep.lines.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["urgent", "addressed", "notable-new", "notable-old", "quiet-old"]);
        assert_eq!(deep.visible().len(), 3);
        assert_eq!(deep.load.live_activities, 5);
        assert_eq!(deep.load.unread_total, 8);
        assert!(deep.load.context_share > 0.0 && deep.load.context_share < 0.2, "{}", deep.load.context_share);
        assert_eq!(deep.loudest().map(|l| l.name.as_str()), Some("urgent"));

        let broad = fold(lines, None, AttentionDial::broad(), 1_000, 10);
        assert_eq!(broad.visible().len(), 5);
        assert!(broad.load.context_share > deep.load.context_share);

        let empty = fold(vec![], None, AttentionDial::default(), 1_000, 10);
        assert!(empty.loudest().is_none());
    }

    // what this catches: the renderer says WHY, in words she can act on, and her own
    // continuation reads first. A strip of numbers without reasons would make her
    // open every room to find out what wanted her.
    #[test]
    fn the_strip_names_the_reason_and_leads_with_her_continuation() {
        let mut l = line("career-wrangler", SalienceLevel::Urgent, 1, 1);
        l.salience.reasons.push(SalienceReason::Blocker { card_id: Uuid::nil(), what: "CI red".into() });
        l.waiting_on_me.push(Uuid::nil());
        let c = Continuation { activity: Uuid::from_u128(0xabcdef), note: "next: run the suite".into(), expectation: None, written_at_ms: 0 };
        let snap = fold(vec![l], Some(c), AttentionDial::default(), 1_000, 1);
        let text = snap.render_lines();
        assert!(text[0].starts_with("[working]"), "{text:?}");
        assert!(text[0].contains("next: run the suite"));
        assert!(text[1].contains("career-wrangler") && text[1].contains("BLOCKER") && text[1].contains("1 waiting on you"), "{text:?}");
    }

    // what this catches: a private continuation (her note, in her mind room) rendered into
    // a PUBLIC turn's strip, and so into that turn's prompt and capture. Outside her mind
    // room the strip says only that private work is pending; inside it, the note reads.
    #[test]
    fn a_private_continuation_shows_its_note_only_in_her_mind_room() {
        let her = Uuid::new_v4();
        let mind = crate::persona::mind_room::mind_room_id(her);
        let c = Continuation { activity: mind, note: "a private plan".into(), expectation: None, written_at_ms: 0 };
        let snap = fold(vec![], Some(c), AttentionDial::default(), 1_000, 1);
        let public = snap.render_lines_in(her, Some(Uuid::new_v4()));
        assert!(!public.concat().contains("a private plan"), "{public:?}");
        assert!(public[0].contains("private work pending"), "{public:?}");
        assert!(snap.render_lines_in(her, Some(mind))[0].contains("a private plan"));
    }
}
