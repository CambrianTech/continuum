//! A module's command wiring the registry refuses: a name claimed twice, or a command
//! constructor shipped without its descriptor. `ModuleRegistry::register` records each one
//! the instant the module registers; boot refuses on any before a client can be told the
//! catalogue (`ModuleRegistry::check_wiring`). The "no central list" design removes the
//! human backstop, so the registry catches its own wiring gaps.

/// One wiring gap, named by the command and module that carry it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WiringDefect {
    /// Two self-registering stateless commands share a name.
    #[error(
        "duplicate stateless command object '{name}'. Command names must be unique across \
         the whole registry."
    )]
    DuplicateStatelessCommand { name: &'static str },
    /// A constructor in a module's `commands()` with no `register_command!` descriptor: it
    /// would route but stay invisible to commands/list, the persona tool surface, the ACL
    /// and codegen.
    #[error(
        "command object '{command}' (module '{module}') has NO DESCRIPTOR — it would route \
         but stay invisible to commands/list, the persona tool surface, the ACL and codegen. \
         Add `crate::register_command!(<Type>);` at its declaration site (descriptor is \
         type-only, so it needs none of the module's deps — see BenchmarkDispatch for the \
         pattern)."
    )]
    MissingDescriptor { command: &'static str, module: &'static str },
    /// A module claims a command name another module already registered.
    #[error(
        "duplicate command object '{command}' — module '{module}' claims a name already \
         registered (prev descriptor: '{previous}'). Command names must be unique across \
         the whole registry."
    )]
    DuplicateCommand { command: &'static str, module: &'static str, previous: &'static str },
    /// A command declaration the descriptor registry could not describe.
    #[error(transparent)]
    Codegen(#[from] crate::sdk_codegen::CodegenDefect),
}

/// Every wiring gap the registry recorded; never empty (`check_wiring` returns `Ok` then).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("ModuleRegistry refused {} wiring defect(s): {}", .0.len(), .0.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))]
pub struct WiringDefects(Vec<WiringDefect>);

impl WiringDefects {
    /// The recorded defects, or `None` when there are none: a `WiringDefects` is never empty.
    pub(crate) fn of(defects: Vec<WiringDefect>) -> Option<Self> {
        (!defects.is_empty()).then_some(Self(defects))
    }

    pub fn defects(&self) -> &[WiringDefect] {
        &self.0
    }
}
