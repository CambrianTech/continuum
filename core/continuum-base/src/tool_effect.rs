//! What a command declares its calls do to the world. Declared once, by the command itself
//! (`CommandSpec::EFFECT` in the kernel), and read by the view's repeat rule (MIND-CONTEXT
//! invariant 4, `continuum-context`). It lives on the floor because both reach it and neither may
//! import the other: the kernel never imports a mind crate, and the view stays a leaf.

/// The effect a command declares. Never inferred from a name, an output or a body: a command that
/// has not declared is `Undeclared`, which authorizes nothing (Codex).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ToolEffect {
    /// It only observes: two calls with one fingerprint and one outcome hash saw the same thing,
    /// so the later one is "outcome unchanged".
    Observes,
    /// It changes the world: every call is its own effect, never a repeat, however alike its
    /// output (two appends that both answer "OK" made two appends). A command that reads and
    /// writes is `Changes`.
    Changes,
    /// It reopens a whole she asked to see again (`tool/output`): always whole, never a repeat,
    /// because she reopens exactly what her view no longer holds.
    Reopens,
    /// The command declares none of these: never a repeat and never taken as observing; nothing
    /// is suppressed, and the replay validator counts these so missing declarations are visible.
    Undeclared,
}

impl ToolEffect {
    /// The effect of an act that made several calls: any change makes it a change; otherwise any
    /// undeclared call leaves it undeclared; otherwise any reopen makes it a reopen; only an act
    /// whose every call observes observes. So one undeclared or changing call can never let the
    /// act count as an unchanged observation.
    pub fn of_calls(effects: impl IntoIterator<Item = ToolEffect>) -> Option<ToolEffect> {
        effects.into_iter().reduce(|a, b| match (a, b) {
            (ToolEffect::Changes, _) | (_, ToolEffect::Changes) => ToolEffect::Changes,
            (ToolEffect::Undeclared, _) | (_, ToolEffect::Undeclared) => ToolEffect::Undeclared,
            (ToolEffect::Reopens, _) | (_, ToolEffect::Reopens) => ToolEffect::Reopens,
            (ToolEffect::Observes, ToolEffect::Observes) => ToolEffect::Observes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an act whose calls mix effects counted as an observation (a read plus a
    // write is a change; a read plus an undeclared call is undeclared), and an act of no calls
    // given an effect at all.
    #[test]
    fn an_act_observes_only_when_every_call_observes() {
        use ToolEffect::*;
        assert_eq!(ToolEffect::of_calls([Observes, Observes]), Some(Observes));
        assert_eq!(ToolEffect::of_calls([Observes, Changes]), Some(Changes));
        assert_eq!(ToolEffect::of_calls([Undeclared, Changes]), Some(Changes));
        assert_eq!(ToolEffect::of_calls([Observes, Undeclared]), Some(Undeclared));
        assert_eq!(ToolEffect::of_calls([Reopens, Undeclared]), Some(Undeclared));
        assert_eq!(ToolEffect::of_calls([Observes, Reopens]), Some(Reopens));
        assert_eq!(ToolEffect::of_calls([]), None);
    }
}
