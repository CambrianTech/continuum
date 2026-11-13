//! What each of the mind's native tools says its result means (ONE-ACT 6b): every native tool
//! projects its outcome (`natives_without_an_outcome` = 0, pinned in core's
//! registry contract), so a failure a verb returns as data never renders as a success.
//!
//! Most of these verbs have no failure state in their result: every failure is a refusal
//! (`Err`), so a result is the work done (`refusals_only`). The one that reports a partial
//! failure as data reads its own result.

use crate::activity::{
    ActivityArchive, ActivityInvite, ActivityInviteResult, ActivityProtect, ActivityRecipes, ActivitySpawn,
};
use crate::commands::focus::{continue_::FocusContinue, mute::FocusMute, nudge::FocusNudge};
use crate::room::{RoomJoin, RoomLeave, RoomList, RoomMembers};
use crate::sdk_codegen::{ProjectsOutcome, ToolVerdict};
use crate::work::submission::{WorkReview, WorkSubmission, WorkSubmit};
use crate::work::{WorkClaim, WorkCreate, WorkGet, WorkHeartbeat, WorkList, WorkNote, WorkRelease, WorkState};

/// An invite fails only when nobody it named joined: one that joined some reports the rest
/// in `failed`, and is the invite done.
impl ProjectsOutcome for ActivityInvite {
    fn outcome(output: &ActivityInviteResult) -> ToolVerdict {
        ToolVerdict::of_success(!output.joined.is_empty() || output.failed.is_empty())
    }
}

crate::register_outcome!(ActivityInvite);
crate::register_outcome!(ActivitySpawn, refusals_only);
crate::register_outcome!(ActivityRecipes, refusals_only);
crate::register_outcome!(ActivityArchive, refusals_only);
crate::register_outcome!(ActivityProtect, refusals_only);
crate::register_outcome!(RoomMembers, refusals_only);
crate::register_outcome!(RoomList, refusals_only);
crate::register_outcome!(RoomJoin, refusals_only);
crate::register_outcome!(RoomLeave, refusals_only);
crate::register_outcome!(WorkClaim, refusals_only);
crate::register_outcome!(WorkCreate, refusals_only);
crate::register_outcome!(WorkGet, refusals_only);
crate::register_outcome!(WorkHeartbeat, refusals_only);
crate::register_outcome!(WorkList, refusals_only);
crate::register_outcome!(WorkNote, refusals_only);
crate::register_outcome!(WorkRelease, refusals_only);
crate::register_outcome!(WorkState, refusals_only);
// A review's `outcome` is its verdict on the work (Passed/Failed), published either way.
crate::register_outcome!(WorkReview, refusals_only);
crate::register_outcome!(WorkSubmission, refusals_only);
crate::register_outcome!(WorkSubmit, refusals_only);
crate::register_outcome!(FocusContinue, refusals_only);
crate::register_outcome!(FocusMute, refusals_only);
crate::register_outcome!(FocusNudge, refusals_only);
