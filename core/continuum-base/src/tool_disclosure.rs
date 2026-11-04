//! Where a command's verb sits in a mind's hands. Declared once, by the command itself
//! (`CommandSpec::DISCLOSURE` in the kernel), beside its access level and its effect, and read by
//! the tool surface that builds her hands (`continuum-cognition`'s `tool_surface`). A value the
//! tool carries like its permissions, never a list of names someone keeps by hand (Joel,
//! 2026-11-04: "values associated with the tool just like perms"). It lives on the floor because
//! the kernel declares it and cognition reads it, and neither may import the other.

/// Where a verb is offered to her. A command that declares nothing is `Core`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Disclosure {
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
    pub fn in_hands(self) -> bool {
        matches!(self, Disclosure::Core | Disclosure::OnDemand)
    }

    /// Whether the verb rides her prompt every act.
    pub fn in_core(self) -> bool {
        self == Disclosure::Core
    }
}
