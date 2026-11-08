//! A declaration the registries cannot describe: a command or event whose contract types
//! ts-rs cannot name, a params schema that did not serialize, or a name claimed twice. The
//! "no central list" design removes the human backstop, so the registries catch these
//! themselves; each is recorded where it is found and boot refuses on any
//! (`ModuleRegistry::check_wiring`), the same refusal as a module's wiring defect.

/// One undescribable declaration, named by the command or event that carries it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodegenDefect {
    /// A command's `Params` is not a named TS type (struct/enum).
    #[error("command '{command}' Params must be a named TS type (struct/enum), not an inline primitive")]
    UnnamedParams { command: &'static str },
    /// A command's `Result` is not a named TS type (struct/enum).
    #[error("command '{command}' Result must be a named TS type (struct/enum), not an inline primitive")]
    UnnamedResult { command: &'static str },
    /// A command's params JSON Schema did not serialize.
    #[error("command '{command}' params schema did not serialize: {error}")]
    ParamsSchema { command: &'static str, error: String },
    /// Two `CommandSpec` impls claim one name.
    #[error(
        "sdk_codegen: duplicate command NAME '{name}' — two CommandSpec impls claim it. \
         Command names must be unique across the whole registry."
    )]
    DuplicateCommand { name: &'static str },
    /// An event's `Payload` is not a named TS type (struct/enum).
    #[error("event '{class}' Payload must be a named TS type (struct/enum)")]
    UnnamedEventPayload { class: &'static str },
    /// Two `EventSpec` impls claim one class.
    #[error("sdk_codegen: duplicate event CLASS '{class}' — two EventSpec impls claim it.")]
    DuplicateEventClass { class: &'static str },
}
