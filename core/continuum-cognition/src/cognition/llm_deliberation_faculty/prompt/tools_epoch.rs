//! Her hands as one claim in the window's one allocation (FLOW-IMPLEMENTATION-PLAN §2d).
//!
//! The tool schemas sit ahead of every message, so a surface that changes mid-epoch rewrites
//! the whole prompt after the system text. The claim therefore prices like her view: at an
//! epoch's start its floor is her core hands and its want her whole surface (hands plus her
//! learned head, `ToolSurface::claim_with_head`); once the epoch has chosen a level, that level
//! is the claim's target (`min_tokens`), which the allocator gives up only when the window
//! cannot hold it (a relaunch onto a smaller window), and then loudly. The floor stays her
//! core hands: a window that cannot hold those refuses the turn, never sends her handless.
//! Before this the surface was sized against a share of the window outside the allocation
//! (`ContextBudget::tool_surface_tokens`), a second budget the allocation never saw.

use super::*;
use crate::cognition::tool_surface::SelectedSurface;

/// The level this epoch of her view chose, in raw (uncalibrated) tokens.
#[derive(Clone, Copy, Debug)]
pub(in super::super) struct ToolsEpoch {
    epoch: u64,
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

impl LlmDeliberationFaculty {
    fn view_epoch(&self) -> Option<u64> {
        self.context_view.lock().as_ref().map(|view| view.epoch_id())
    }

    /// The tools claim. Demand never depends on a grant.
    pub(in super::super) fn tools_claim(&self, ws: &Workspace) -> ToolsClaim {
        let (core, want) = self.tool_claim_tokens(ws);
        let retained = *self.tools_epoch.lock();
        let level = match (retained, self.view_epoch()) {
            (Some(kept), Some(epoch)) if kept.epoch == epoch => kept.tokens.max(core),
            _ => core,
        };
        ToolsClaim { floor: core, level, want: want.max(level) }
    }

    /// The surface the allocation's grant buys, or the loud refusal when the window could not
    /// grant even the floor (never a silent shrink below her hands).
    pub(in super::super) fn granted_tools(&self, ws: &Workspace, claim: ToolsClaim, grant: usize) -> (SelectedSurface<'_>, Option<PromptCapacityError>) {
        if grant < claim.level {
            continuum_base::probe!(
                class = "delib.tools.epoch_level_lost",
                persona = %self.persona_name,
                level = claim.level as u64,
                grant = grant as u64,
                "the window could not hold the tool level this epoch chose: her hands change mid-epoch and the prompt after the system text re-prefills"
            );
        }
        match self.select_tool_surface_granted(ws, grant) {
            Ok(selected) => (selected, None),
            Err(needed) => (
                SelectedSurface::none(),
                Some(PromptCapacityError { required_tokens: needed, budget_tokens: grant }),
            ),
        }
    }

    /// After her view is fitted: the level this act sent is the level of the epoch the view is
    /// now in, so an epoch this act started keeps it, and the next start chooses again.
    pub(in super::super) fn retain_tools(&self, selected: &SelectedSurface<'_>, refused: bool) {
        if refused {
            return; // a refused turn sent no surface: the epoch keeps the level it had
        }
        if let Some(epoch) = self.view_epoch() {
            *self.tools_epoch.lock() = Some(ToolsEpoch { epoch, tokens: selected.tokens });
        }
    }
}
