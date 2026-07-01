//! `cognition/<verb>` — the cognition command family (typed, self-routing).
//!
//! These are the persona cognitive-pipeline commands migrated off the legacy
//! `CognitionModule::handle_command` match onto the typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand) path. The gating/generation
//! decisions here call free functions in [`crate::cognition`] (no `CognitionModule`
//! state), so each is a **stateless** `action_command!` unit struct: `inventory`
//! publishes both the descriptor and the runtime object, no module `commands()`
//! ceremony. Stateful cognition commands (per-persona genome, inbox, engram store)
//! migrate as dep-holding families in later slices.
//!
//! Everything here is `access: Internal` — substrate cognition IPC the host drives,
//! registered and grid-routable but not remote-callable persona toolbelt verbs.

pub mod check_redundancy;
pub mod should_respond;
