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
//! # The epoch keeps the set, not only its level
//!
//! Keeping the level alone still re-selected the set every act from that act's affordances (a
//! dirty worktree offers `git_diff`, a held card `get_task`): Aiko on the M5, 2026-10-29, 19 of
//! 89 requests changed the head, every one in the tool list, a full re-prefill on ~21% of her
//! acts. So the act that starts an epoch selects (a start in `place_view`, or a turnover in
//! `fit_view`, which adopts the selection this act made from its own affordances) and every
//! later act of the epoch sends exactly that set, in the same order ([`ToolsEpoch`]). The set
//! changes mid-epoch only when the window cannot hold its level (`delib.tools.epoch_level_lost`,
//! then re-selected and retained).
//!
//! The trade, stated: a verb whose affordance appears mid-epoch is not in her head until the next
//! start. On a server that takes `tail_tools` her learned head's loaded verbs still ride the tail
//! every act (the tail sits after the messages, never in the prefix), so a verb she loads stays
//! callable at once; on one that does not, the verb waits for the next start, and `commands/help`
//! plus the executor's dispatch by name still reach it. A retained verb whose affordance vanished
//! stays in the set until the next start: its command refuses a call that no longer applies.

use super::*;
use crate::cognition::tool_surface::{SelectedSurface, SurfaceReason};

/// The selection this epoch of her view chose: its verbs as canonical names in the order sent,
/// why, and its level in raw (uncalibrated) tokens.
#[derive(Clone, Debug)]
pub(in super::super) struct ToolsEpoch {
    epoch: u64,
    names: Vec<String>,
    reason: SurfaceReason,
    tokens: usize,
}

/// This act's tools claim, in raw tokens.
#[derive(Clone, Copy, Debug)]
pub(in super::super) struct ToolsClaim {
    /// Her core hands: granted unconditionally or the turn is refused.
    pub(in super::super) floor: usize,
    /// The level this epoch chose (her core hands at an epoch's start): held unless the
    /// window cannot.
    pub(in super::super) level: usize,
    /// Her whole surface: what demand records, whatever the grant.
    pub(in super::super) want: usize,
}

/// This act's hands before her view is fitted ([`LlmDeliberationFaculty::retain_tools`] decides
/// what goes out once it is known whether this act turned the view over).
pub(in super::super) struct ToolsChoice<'a> {
    /// What the budget charges: the epoch's retained set while the window holds it, charged at
    /// least `fresh`'s size so a turnover in this act can adopt `fresh`; else `fresh` itself.
    pub(in super::super) sent: SelectedSurface<'a>,
    /// Selected from this act's affordances at the same grant, beside a retained `sent`.
    fresh: Option<SelectedSurface<'a>>,
    /// Whether `sent` is the epoch's retained set.
    kept: bool,
    /// Her view's epoch when this act selected.
    epoch: Option<u64>,
}

impl LlmDeliberationFaculty {
    fn view_epoch(&self) -> Option<u64> {
        self.context_view.lock().as_ref().map(|view| view.epoch_id())
    }

    /// The tools claim. Demand never depends on a grant.
    pub(in super::super) fn tools_claim(&self, ws: &Workspace) -> ToolsClaim {
        let (core, want) = self.tool_claim_tokens(ws);
        let retained = self.tools_epoch.lock().as_ref().map(|kept| (kept.epoch, kept.tokens));
        let level = match (retained, self.view_epoch()) {
            (Some((epoch, tokens)), Some(now)) if epoch == now => tokens.max(core),
            _ => core,
        };
        ToolsClaim { floor: core, level, want: want.max(level) }
    }

    /// The surface the allocation's grant buys: the epoch's retained set while the grant holds
    /// it, else a selection from this act's affordances, or the loud refusal when the window
    /// could not grant even the floor (never a silent shrink below her hands).
    pub(in super::super) fn granted_tools(&self, ws: &Workspace, claim: ToolsClaim, grant: usize) -> (ToolsChoice<'_>, Option<PromptCapacityError>) {
        let epoch = self.view_epoch();
        let retained = self.tools_epoch.lock().as_ref().filter(|kept| Some(kept.epoch) == epoch).cloned();
        let kept = match retained {
            Some(kept) if kept.tokens <= grant => Some(self.surface.retained(&kept.names, kept.reason, kept.tokens)),
            Some(kept) => {
                continuum_base::probe!(
                    class = "delib.tools.epoch_level_lost",
                    persona = %self.persona_name,
                    level = kept.tokens as u64,
                    claim_level = claim.level as u64,
                    grant = grant as u64,
                    "the window could not hold the tool set this epoch chose: her hands change mid-epoch and the prompt after the system text re-prefills"
                );
                None
            }
            None => None,
        };
        // Selected every act, so her learned head computes and reserves the head the next epoch
        // would hold (`with_learned_head`) and a turnover in this act has a set to adopt.
        let fresh = self.select_tool_surface_granted(ws, grant);
        match (kept, fresh) {
            (Some(mut sent), fresh) => {
                let fresh = fresh.ok();
                let want = self.tool_claim_tokens(ws).1;
                if let Some(fresh) = fresh.as_ref() {
                    sent.tail = fresh.tail.clone(); // the verbs she loaded ride the tail, outside the prefix
                    sent = sent.reserving(fresh.tokens);
                }
                (ToolsChoice { sent: sent.with_demand(want), fresh, kept: true, epoch }, None)
            }
            (None, Ok(selected)) => (ToolsChoice { sent: selected, fresh: None, kept: false, epoch }, None),
            (None, Err(needed)) => (
                ToolsChoice { sent: SelectedSurface::none(), fresh: None, kept: false, epoch },
                Some(PromptCapacityError { required_tokens: needed, budget_tokens: grant }),
            ),
        }
    }

    /// After her view is fitted: the surface this act sends, and the set of the epoch the view
    /// is now in. Mid-epoch the retained set goes out unchanged. An act whose view is in an epoch
    /// with no set yet (it started or turned over here, or the window lost the set) retains what
    /// it sends; a turnover in this act sends this act's own selection, charged what the budget
    /// already reserved for it.
    pub(in super::super) fn retain_tools<'a>(&'a self, choice: ToolsChoice<'a>, refused: bool) -> SelectedSurface<'a> {
        let ToolsChoice { sent, fresh, kept, epoch: chosen_in } = choice;
        if refused {
            return sent; // a refused turn sent no surface: the epoch keeps the set it had
        }
        let Some(now) = self.view_epoch() else {
            return sent;
        };
        let turned_over = chosen_in != Some(now);
        if kept && !turned_over {
            return sent;
        }
        let (sending, level) = match fresh {
            Some(fresh) if kept && turned_over => {
                let level = fresh.tokens;
                (fresh.reserving(sent.tokens), level)
            }
            _ => {
                let level = sent.tokens;
                (sent, level)
            }
        };
        *self.tools_epoch.lock() = Some(ToolsEpoch {
            epoch: now,
            names: self.surface.names_of(&sending),
            reason: sending.reason,
            tokens: level,
        });
        sending
    }
}
