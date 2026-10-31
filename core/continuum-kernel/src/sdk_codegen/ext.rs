//! The live command catalogue: the compile-time registry plus every authored manifest's
//! descriptor. The kernel owns the union; loading and running authored commands (manifests, the
//! process they spawn) is `modules::ext_commands`, which installs the descriptors at boot.

use super::CommandDescriptor;

static EXT_DESCRIPTORS: std::sync::OnceLock<Vec<CommandDescriptor>> = std::sync::OnceLock::new();

/// Install the manifests' descriptors for this process. Called ONCE at boot, before
/// any catalogue reader memoizes (the ACL's command sets, the tool dialect index).
/// A second install is a boot-order bug, reported and ignored.
pub fn install_ext_descriptors(descriptors: Vec<CommandDescriptor>) {
    if EXT_DESCRIPTORS.set(descriptors).is_err() {
        tracing::error!(target: "ext", "authored command descriptors installed twice — the first install stands");
    }
}

/// THE ONE UNION POINT: every command this node can route — the compile-time registry
/// plus every authored manifest — sorted by name. Every catalogue reader uses this;
/// codegen alone keeps reading [`super::command_registry`].
pub fn command_registry_live() -> Vec<CommandDescriptor> {
    let mut all = super::command_registry();
    if let Some(ext) = EXT_DESCRIPTORS.get() {
        all.extend(ext.iter().cloned());
        all.sort_by(|a, b| a.name.cmp(b.name));
    }
    all
}


/// What the command named `canonical` declares its calls do ([`super::CommandSpec::EFFECT`]),
/// for a name already resolved through the dispatch seam (`tool_dialect::resolve_wire_name`),
/// so the effect is the descriptor's of the command that runs, never read off what the model
/// typed. A name no command claims is `Undeclared`: it authorizes nothing. Built once from the
/// live catalogue, which is immutable after boot's one install.
pub fn effect_of(canonical: &str) -> continuum_base::tool_effect::ToolEffect {
    static EFFECTS: std::sync::OnceLock<std::collections::HashMap<&'static str, continuum_base::tool_effect::ToolEffect>> =
        std::sync::OnceLock::new();
    let effects = EFFECTS.get_or_init(|| command_registry_live().iter().map(|d| (d.name, d.effect)).collect());
    match effects.get(canonical) {
        Some(effect) => *effect,
        None => continuum_base::tool_effect::ToolEffect::Undeclared,
    }
}
