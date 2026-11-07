//! Where a command's verb sits in a mind's hands. Declared once, by the command itself
//! (`CommandSpec::DISCLOSURE` in the kernel), beside its access level and its effect, and read by
//! the tool surface that builds her hands (`continuum-cognition`'s `tool_surface`). A value the
//! tool carries like its permissions, never a list of names someone keeps by hand (Joel,
//! 2026-11-04: "values associated with the tool just like perms"). It lives on the floor because
//! the kernel declares it and cognition reads it, and neither may import the other.

/// Where a verb is offered to her. A command that declares nothing is `Core`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Disclosure {
    /// In every surface she is offered, a room whose recipe narrows her hands included: the
    /// turn's own protocol (her verdict verbs) and the discovery pair that reaches everything
    /// else (Kimi, 2026-10-05: "help works; nothing to invoke against").
    Always,
    /// In her prompt every act: one of her core hands.
    #[default]
    Core,
    /// In her hands but not her prompt: one `commands/help` away, and in her prompt once her learned
    /// head holds it, so a rarely used verb never takes her view's window.
    OnDemand,
    /// Never in her hands: a verb whose misuse costs more than its reach (its guidance says why and
    /// what to use instead, `tool_dialect::withheld_from_hands`). It stays callable through the
    /// catalog.
    Withheld,
}

impl Disclosure {
    /// Whether the verb is one of her hands at all (in her prompt or one `commands/help` away).
    pub const fn in_hands(self) -> bool {
        matches!(self, Disclosure::Always | Disclosure::Core | Disclosure::OnDemand)
    }

    /// Whether the verb rides her prompt every act.
    pub const fn in_core(self) -> bool {
        matches!(self, Disclosure::Always | Disclosure::Core)
    }

    /// Whether the verb rides every surface, a room's declared one included.
    pub const fn in_every_surface(self) -> bool {
        matches!(self, Disclosure::Always)
    }
}
