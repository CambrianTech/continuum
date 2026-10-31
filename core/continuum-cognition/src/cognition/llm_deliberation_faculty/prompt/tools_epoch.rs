//! Her hands as one claim in the window's one allocation (FLOW-IMPLEMENTATION-PLAN §2d), and
//! one SET per epoch of her view (plan goal 10: tools are working sets).
//!
//! The tool schemas sit ahead of every message, so a surface that changes mid-epoch rewrites
//! the whole prompt after the system text. The claim therefore prices like her view: at an
//! epoch's start its floor is her core hands and its want her whole surface (hands plus her
//! learned head, `ToolSurface::claim_with_head`); once the epoch has chosen a level, that level
//! is the claim's target (`min_tokens`), which the allocator gives up only when the window
//! cannot hold it (a relaunch onto a smaller window), and then loudly. The floor stays her
//! core hands: a window that cannot hold those refuses the turn, never sends her handless.
//!
//! # The set outlives her view's turnovers
//!
//! Keeping the level alone still re-selected the set every act from that act's affordances (a
//! dirty worktree offers `git_diff`, a held card `get_task`): Aiko on the M5, 2026-10-29, 19 of
//! 89 requests changed the head, every one in the tool list. Holding the set per epoch of her
//! VIEW still re-selected it at every turnover, from that act's grant, and a turnover starts the
//! claim at her core hands: on the M5, 2026-10-31 on 526bbe418, Aris's set flipped between her
//! 38-verb hands and her 23-verb core within one room, and every cold prefill in two hours (13 of
//! 13, Aris and Benchy, ~40k tokens at 100+ s each) followed a head change, while every restore
//! behind an unchanged head came back warm. A view turnover re-renders only what follows the
//! head; re-selecting the head there turned a view re-prefill into a whole one.
//!
//! So the set is held across her view's turnovers ([`ToolsEpoch`]) and changes only:
//! - down, when the window cannot hold it (`delib.tools.epoch_level_lost`), then re-selected and
//!   held;
//! - up, once, from her core hands (`SurfaceReason::CoreHandsForWindow`, the window's fallback)
//!   to a fuller selection that now fits (`delib.tools.head_restored`), never back and forth;
//! - when her surface is rebuilt (`rebuild_tool_surface` clears it).
//!
//! The trade, stated: a verb whose affordance appears later, or that her learned head would add,
//! is not in her head until one of those changes. On a server that takes `tail_tools` her learned
//! head's loaded verbs still ride the tail every act (after the messages, never in the prefix),
//! so a verb she loads stays callable at once; on one that does not, `commands/help` plus the
//! executor's dispatch by name still reach it. A held verb whose affordance vanished stays in the
//! set: its command refuses a call that no longer applies.

use super::*;
use crate::cognition::tool_surface::{SelectedSurface, SurfaceReason};

/// The set her head holds: its verbs as canonical names in the order sent, why, and its level in
/// raw (uncalibrated) tokens.
#[derive(Clone, Debug)]
pub(in super::super) struct ToolsEpoch {
    names: Vec<String>,
    reason: SurfaceReason,
    tokens: usize,
}

/// This act's tools claim, in raw tokens.
#[derive(Clone, Copy, Debug)]
pub(in super::super) struct ToolsClaim {
    /// Her core hands: granted unconditionally or the turn is refused.
    pub(in super::super) floor: usize,
    /// The level her held set costs (her core hands before any set is held): held unless the
    /// window cannot.
    pub(in super::super) level: usize,
    /// Her whole surface: what demand records, whatever the grant.
    pub(in super::super) want: usize,
}

/// This act's hands before her view is fitted ([`LlmDeliberationFaculty::retain_tools`] decides
/// what goes out once it is known whether this act turned the view over).
pub(in super::super) struct ToolsChoice<'a> {
    /// What the budget charges and the request sends: her held set while the window holds it,
    /// else this act's selection.
    pub(in super::super) sent: SelectedSurface<'a>,
    /// Whether `sent` is her held set.
    kept: bool,
}

impl LlmDeliberationFaculty {
    /// The tools claim. Demand never depends on a grant.
    pub(in super::super) fn tools_claim(&self, ws: &Workspace) -> ToolsClaim {
        let (core, want) = self.tool_claim_tokens(ws);
        let level = self.tools_epoch.lock().as_ref().map_or(core, |held| held.tokens.max(core)); // map_or: no set held yet claims her core hands
        ToolsClaim { floor: core, level, want: want.max(level) }
    }

    /// The surface the allocation's grant buys: her held set while the grant holds it (or the
    /// fuller set that replaces her core-hands fallback once it fits), else a selection from
    /// this act's affordances, or the loud refusal when the window could not grant even the
    /// floor (never a silent shrink below her hands).
    pub(in super::super) fn granted_tools(&self, ws: &Workspace, claim: ToolsClaim, grant: usize) -> (ToolsChoice<'_>, Option<PromptCapacityError>) {
        let held = self.tools_epoch.lock().clone();
        // Selected every act, so her learned head computes and reserves the head a later set
        // would hold (`with_learned_head`) and its loaded verbs ride the tail.
        let fresh = self.select_tool_surface_granted(ws, grant);
        // Up, once: her core-hands fallback gives way to the fuller set the window now holds.
        let fallback_held = held.as_ref().filter(|held| held.tokens <= grant && held.reason == SurfaceReason::CoreHandsForWindow).map(|held| held.tokens);
        let fuller = fresh.as_ref().ok().filter(|fresh| fresh.reason != SurfaceReason::CoreHandsForWindow).map(|fresh| fresh.tokens);
        if let (Some(from), Some(to)) = (fallback_held, fuller) {
            continuum_base::probe!(
                class = "delib.tools.head_restored",
                persona = %self.persona_name,
                from = from as u64,
                to = to as u64,
                "her core-hands fallback is replaced by the fuller set the window now holds: one head change, up"
            );
            if let Ok(fuller) = fresh {
                return (ToolsChoice { sent: fuller, kept: false }, None);
            }
        }
        let kept = match held {
            Some(held) if held.tokens <= grant => Some(self.surface.retained(&held.names, held.reason, held.tokens)),
            Some(held) => {
                continuum_base::probe!(
                    class = "delib.tools.epoch_level_lost",
                    persona = %self.persona_name,
                    level = held.tokens as u64,
                    claim_level = claim.level as u64,
                    grant = grant as u64,
                    "the window could not hold her tool set: her head changes and the whole prompt re-prefills"
                );
                None
            }
            None => None,
        };
        match (kept, fresh) {
            (Some(mut sent), fresh) => {
                let want = self.tool_claim_tokens(ws).1;
                if let Ok(fresh) = fresh.as_ref() {
                    sent.tail = fresh.tail.clone(); // the verbs she loaded ride the tail, outside the prefix
                }
                (ToolsChoice { sent: sent.with_demand(want), kept: true }, None)
            }
            (None, Ok(selected)) => (ToolsChoice { sent: selected, kept: false }, None),
            (None, Err(needed)) => (
                ToolsChoice { sent: SelectedSurface::none(), kept: false },
                Some(PromptCapacityError { required_tokens: needed, budget_tokens: grant }),
            ),
        }
    }

    /// After her view is fitted: the surface this act sends. Her held set goes out unchanged,
    /// whether or not her view turned over in this act; an act with no held set (the first, a
    /// lost level, or the one upward restore) holds what it sends.
    pub(in super::super) fn retain_tools<'a>(&'a self, choice: ToolsChoice<'a>, refused: bool) -> SelectedSurface<'a> {
        let ToolsChoice { sent, kept } = choice;
        if refused || kept {
            return sent; // a refused turn sent no surface; a held set stays held
        }
        *self.tools_epoch.lock() = Some(ToolsEpoch {
            names: self.surface.names_of(&sent),
            reason: sent.reason,
            tokens: sent.tokens,
        });
        sent
    }
}
