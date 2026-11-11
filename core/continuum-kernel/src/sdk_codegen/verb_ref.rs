//! A verb named in text a mind reads, and the one function that names a command on the wire.
//!
//! Every handle, marker and refusal the substrate shows her names the verb that acts on it; a
//! name she cannot call is a handle she cannot open (2026-11-06, the M5: `tool/output` was named
//! by every folded act, saved result and thought, was not native, and no act called it; and the
//! slash form it was named by is not the name her tool list offers, which is each command's first
//! alias, `read_output`). So text never spells a verb. It renders a [`VerbRef`]:
//!
//! - it points at the command's TYPE, never a copy of its name, so a rename re-renders every
//!   reference and a moved or removed command fails to compile where it is named;
//! - it can only be built from a command in her hands (checked when the constant is evaluated);
//! - it renders through [`wire_name`], the same function her offered tool list is named by, so
//!   the verb in her text is the verb in her function list.
//!
//! A rename keeps the old name as a declared alias (`ALIASES`), so her old calls still resolve.
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
//! // Rendered as her tool list names it: the first alias under the trained-reflex offer.
//! assert_eq!(format!("reopen with {REOPEN}"), "reopen with reopen_output");
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

/// How a command is NAMED on the wire when offered to a model. Both the offered form and every
/// declared alias resolve on the way back in, so a style only changes what she is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferStyle {
    /// Offer the model's trained reflex: the command's primary alias (`code/read` → `read_file`).
    TrainedReflex,
    /// Offer our canonical name, charset-legal (`code/read` → `code_read`).
    Canonical,
}

/// The offer policy for a served model: the per-model seam. Today every model is met with its
/// training (the residents' coder LoRAs are tuned on the trained names); `Canonical` is adopted
/// here, once, only after a tool-using A/B validates it (#204).
pub fn offer_style_for(_model: Option<&str>) -> OfferStyle {
    OfferStyle::TrainedReflex
}

/// A command's primary alias: the first it declares, the trained reflex it is offered under.
pub fn primary_alias(aliases: &'static [&'static str]) -> Option<&'static str> {
    aliases.first().copied()
}

/// THE wire name of a command under `style`: its primary alias, or its canonical name made
/// charset-legal (`/` → `_`) when it has none or the style is canonical. Never a raw slash. Her
/// offered tool list and every verb her text names go through this one function.
pub fn wire_name(name: &str, primary_alias: Option<&str>, style: OfferStyle) -> String {
    match (style, primary_alias) {
        (OfferStyle::TrainedReflex, Some(alias)) => alias.to_owned(),
        _ => name.replace('/', "_"),
    }
}

/// The name her tool list offers a command under, read from its declaration (canonical name
/// and aliases) through [`wire_name`] and the served model's [`offer_style_for`]. Every verb and
/// example call her text names is offered through this, so none can disagree with her tools.
pub fn offered_name_of(name: &str, aliases: &'static [&'static str]) -> String {
    wire_name(name, primary_alias(aliases), offer_style_for(None))
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

    /// The name her tool list offers it under, read from the declaration when rendered.
    pub fn offered(self) -> String {
        offered_name_of(C::NAME, C::ALIASES)
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
