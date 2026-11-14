//! A verb named in text a mind reads, and the one function that names a command on the wire.
//!
//! Every handle, marker and refusal the substrate shows her names the verb that acts on it; a
//! name she cannot call is a handle she cannot open (2026-11-06, the M5: `tool/output` was named
//! by every folded act, saved result and thought, was not native, and no act called it). So text
//! never spells a verb. It renders a [`VerbRef`]:
//!
//! - it points at the command's TYPE, never a copy of its name, so a rename re-renders every
//!   reference and a moved or removed command fails to compile where it is named;
//! - it can only be built from a command in her hands (checked when the constant is evaluated);
//! - it renders through [`wire_name`], the same function her offered tool list is named by, so
//!   the verb in her text is the verb in her function list.
//!
//! ONE NAME PER VERB, derived from its command path (card d2b6dbce): `work/release` is offered as
//! `work_release`, for every command, by [`wire_name`] alone. A command's `ALIASES` are other
//! names it ANSWERS to (the trained reflexes a model types, `read_file`, `bash`), resolved
//! inbound by `tool_dialect`; they never rename what she is offered. Before this, the first alias
//! renamed the offer one verb at a time, so one family reached her under two schemes
//! (`claim_task`, `release_task` beside `work_create`, `work_submit`): Aiko on the M5,
//! 2026-11-14 04:55Z, reasoned in `work_*` names, found no `work_release`, and called
//! `work_create` again ("I'm stuck in a loop creating the same duplicate card again").
//!
//! Two tiers, by where the text can appear:
//! - [`VerbRef::in_hands`]: native and in her hands (`Always`, `Core` or `OnDemand`), for text
//!   shown where her full hands are offered;
//! - [`VerbRef::always`]: also in every surface, a room's narrowed one included
//!   (`Disclosure::Always`), for a handle, which can appear in any surface she is offered.
//!
//! Declare each as a `const` item, so the check runs at compile time (`cargo check` included):
//! ```
//! # use continuum_kernel::sdk_codegen::*;
//! # #[derive(serde::Deserialize, ts_rs::TS, schemars::JsonSchema)] pub struct P { x: String }
//! # #[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)] pub struct R { success: bool }
//! continuum_kernel::action_command! {
//!     /// Reopen a saved whole.
//!     pub struct Reopen;
//!     name: "doc/reopen",
//!     access: AiSafe,
//!     native: true,
//!     aliases: &["reopen_output"],
//!     disclosure: Always,
//!     params: P,
//!     output: R,
//!     run(_this, _ctx, _p) => { Ok(R { success: true }) }
//! }
//! impl ProjectsOutcome for Reopen {
//!     fn outcome(r: &R) -> ToolVerdict { ToolVerdict::of_success(r.success) }
//! }
//! const REOPEN: VerbRef<Reopen> = VerbRef::always();
//! // Rendered as her tool list names it: derived from its path, never from an alias.
//! assert_eq!(format!("reopen with {REOPEN}"), "reopen with doc_reopen");
//! ```
//! A catalog-only command cannot be named (what this catches: a marker naming a verb that is not
//! on her native surface, `tool/output` before 2026-11-06):
//! ```compile_fail,E0080
//! # use continuum_kernel::sdk_codegen::*;
//! # #[derive(serde::Deserialize, ts_rs::TS, schemars::JsonSchema)] pub struct P { x: String }
//! # #[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)] pub struct R { success: bool }
//! continuum_kernel::action_command! {
//!     /// Reopen a saved whole.
//!     pub struct Reopen;
//!     name: "doc/reopen",
//!     access: AiSafe,
//!     params: P,
//!     output: R,
//!     run(_this, _ctx, _p) => { Ok(R { success: true }) }
//! }
//! const REOPEN: VerbRef<Reopen> = VerbRef::in_hands();
//! # fn main() { let _ = REOPEN; }
//! ```
//! and a handle cannot name a native verb that a room's narrowed surface can drop (what this
//! catches: a handle shown in a room whose declared hands do not hold its verb):
//! ```compile_fail,E0080
//! # use continuum_kernel::sdk_codegen::*;
//! # #[derive(serde::Deserialize, ts_rs::TS, schemars::JsonSchema)] pub struct P { x: String }
//! # #[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)] pub struct R { success: bool }
//! continuum_kernel::action_command! {
//!     /// Reopen a saved whole.
//!     pub struct Reopen;
//!     name: "doc/reopen",
//!     access: AiSafe,
//!     native: true,
//!     disclosure: OnDemand,
//!     params: P,
//!     output: R,
//!     run(_this, _ctx, _p) => { Ok(R { success: true }) }
//! }
//! impl ProjectsOutcome for Reopen {
//!     fn outcome(r: &R) -> ToolVerdict { ToolVerdict::of_success(r.success) }
//! }
//! const REOPEN: VerbRef<Reopen> = VerbRef::always();
//! # fn main() { let _ = REOPEN; }
//! ```

use std::marker::PhantomData;

use super::CommandSpec;
use continuum_base::tool_disclosure::Disclosure;

/// THE wire name of a command: its command path made charset-legal (`/` → `_`), so
/// `work/release` is `work_release`. Every command her tool list offers, and every verb her text
/// names, is named by this one function, so the name she reasons in for one member of a family is
/// the scheme of every member, and a verb she infers from its siblings exists. Declared aliases
/// never enter it: they only widen what resolves inbound (`tool_dialect::resolve_wire_name`).
pub fn wire_name(name: &str) -> String {
    name.replace('/', "_")
}

/// A reference to a command in her hands, by its type. Renders as her tool list names it.
pub struct VerbRef<C>(PhantomData<fn() -> C>);

impl<C> Clone for VerbRef<C> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C> Copy for VerbRef<C> {}

impl<C: CommandSpec> VerbRef<C> {
    /// A native verb in her hands. Use in a `const` item: a command that is catalog-only or
    /// `Withheld` fails the constant's evaluation.
    pub const fn in_hands() -> Self {
        assert!(C::NATIVE, "a verb named in her text must be native: declare `native: true` on the command");
        assert!(C::DISCLOSURE.in_hands(), "a verb named in her text must be in her hands, never `Withheld`");
        Self(PhantomData)
    }

    /// A native verb in every surface she is offered (`Disclosure::Always`): the only kind a
    /// handle may name, because a handle can appear in a room whose declared hands are narrowed.
    pub const fn always() -> Self {
        let verb = Self::in_hands();
        assert!(C::DISCLOSURE.in_every_surface(), "a verb a handle names must declare `disclosure: Always`");
        verb
    }

    /// The canonical command name, for matching a resolved call; never for her text.
    pub fn canonical(self) -> &'static str {
        C::NAME
    }

    /// Where the command declares it sits in her hands.
    pub fn disclosure(self) -> Disclosure {
        C::DISCLOSURE
    }

    /// The name her tool list offers it under ([`wire_name`] of its path).
    pub fn offered(self) -> String {
        wire_name(C::NAME)
    }
}

impl<C: CommandSpec> std::fmt::Display for VerbRef<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.offered())
    }
}

impl<C> std::fmt::Debug for VerbRef<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(std::any::type_name::<C>())
    }
}
