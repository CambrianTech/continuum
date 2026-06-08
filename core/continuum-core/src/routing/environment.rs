//! `EnvironmentId` — the typed embodiment dimension of the URI grammar.
//!
//! Per `docs/architecture/GRID-ADDRESSING-AND-ROUTING.md` (Slice P): the
//! `:env` component of `airc://<peer>[:env]/<path>` selects which
//! presentation context handles the dispatch. An actor can have
//! multiple environments active simultaneously (Joel's browser tab,
//! VR headset, terminal session all live at once); this type names
//! them.
//!
//! ## Variants
//!
//! - [`EnvironmentId::Named`] — specific environment by name. The
//!   well-known names (`web`, `tty`, `cli`, `vr`, `ar`, `headless`)
//!   are convention; the type accepts any non-empty string so
//!   custom envs slot in without substrate changes.
//! - [`EnvironmentId::Wildcard`] — match every active environment
//!   of the target. Used by broadcast dispatch (the `:*` URI form).
//!
//! ## Custom env naming convention
//!
//! Per the design doc's open-questions resolution lean: custom env
//! names SHOULD be prefixed with `x-<vendor>-<name>` (e.g.
//! `x-cambriantech-bevy-rig`) to avoid colliding with future
//! well-known names. The parser doesn't enforce the prefix today;
//! enforcement lands when the env registry adds collision detection.

use std::fmt;

/// The substrate's typed environment selector.
///
/// `Named("web")` and `Named("vr")` are NOT the same env even though
/// they're parameterized by the same variant — environments are
/// distinct by their full string name. Per [[no-fallbacks-ever]] the
/// substrate never coerces unknown env names to a default; it
/// either has the env registered or returns a typed routing failure.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EnvironmentId {
    /// Specific environment. Well-known: `web`, `tty`, `cli`,
    /// `vr`, `ar`, `headless`. Custom: any non-empty string,
    /// recommended prefix `x-<vendor>-<name>`.
    Named(String),
    /// Match every active environment of the target actor. Surfaces
    /// in the URI grammar as `:*`.
    Wildcard,
}

impl EnvironmentId {
    /// Convenience constructor for the well-known names. Equivalent
    /// to `Named(name.into())` but spelled to read at call sites
    /// like the URI it parses from.
    pub fn well_known(name: WellKnownEnv) -> Self {
        EnvironmentId::Named(name.as_str().to_string())
    }

    /// Borrow the name string if this is a `Named` env; `None` for
    /// `Wildcard`. Useful when the caller wants to compare without
    /// match-on-variant ceremony.
    pub fn name(&self) -> Option<&str> {
        match self {
            EnvironmentId::Named(n) => Some(n.as_str()),
            EnvironmentId::Wildcard => None,
        }
    }

    /// `true` iff this selector matches every env on the target.
    pub fn is_wildcard(&self) -> bool {
        matches!(self, EnvironmentId::Wildcard)
    }
}

impl fmt::Display for EnvironmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvironmentId::Named(n) => write!(f, "{n}"),
            EnvironmentId::Wildcard => write!(f, "*"),
        }
    }
}

/// Well-known environment names recognized by the substrate. Custom
/// envs go through `EnvironmentId::Named(String)` directly with
/// the `x-<vendor>-<name>` prefix convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WellKnownEnv {
    /// Browser DOM / web shell.
    Web,
    /// Terminal-based interactive UI (menu-based TUI).
    Tty,
    /// Non-interactive CLI invocation (`./jtag`, scripts).
    Cli,
    /// VR scene (Bevy, OpenXR, etc.).
    Vr,
    /// AR overlay.
    Ar,
    /// No presentation; substrate-only consumer (Ares, sentinels, foundry).
    Headless,
}

impl WellKnownEnv {
    /// Canonical lowercase name as it appears in URI authority.
    pub const fn as_str(&self) -> &'static str {
        match self {
            WellKnownEnv::Web => "web",
            WellKnownEnv::Tty => "tty",
            WellKnownEnv::Cli => "cli",
            WellKnownEnv::Vr => "vr",
            WellKnownEnv::Ar => "ar",
            WellKnownEnv::Headless => "headless",
        }
    }

    /// All well-known envs, useful for registry seeding and tests.
    pub const fn all() -> &'static [WellKnownEnv] {
        &[
            WellKnownEnv::Web,
            WellKnownEnv::Tty,
            WellKnownEnv::Cli,
            WellKnownEnv::Vr,
            WellKnownEnv::Ar,
            WellKnownEnv::Headless,
        ]
    }
}

impl fmt::Display for WellKnownEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Infallible `WellKnownEnv -> EnvironmentId` conversion. Lets call
/// sites pass the typed enum directly where an `EnvironmentId` is
/// expected.
impl From<WellKnownEnv> for EnvironmentId {
    fn from(w: WellKnownEnv) -> Self {
        EnvironmentId::Named(w.as_str().to_string())
    }
}

/// Migration shim for `&str` -> `EnvironmentId`, mirroring the
/// `From<&str> for CommandUri` pattern. `"*"` becomes `Wildcard`;
/// anything else becomes `Named`. Allows ergonomic conversions at
/// call sites that have an env name as a string.
impl From<&str> for EnvironmentId {
    fn from(s: &str) -> Self {
        if s == "*" {
            EnvironmentId::Wildcard
        } else {
            EnvironmentId::Named(s.to_string())
        }
    }
}

impl From<String> for EnvironmentId {
    fn from(s: String) -> Self {
        EnvironmentId::from(s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_env_round_trips_via_display() {
        let env = EnvironmentId::Named("web".into());
        assert_eq!(env.to_string(), "web");
        assert_eq!(EnvironmentId::from(env.to_string()), env);
    }

    #[test]
    fn wildcard_env_round_trips_via_display() {
        let env = EnvironmentId::Wildcard;
        assert_eq!(env.to_string(), "*");
        assert_eq!(EnvironmentId::from(env.to_string()), env);
    }

    #[test]
    fn from_str_recognizes_wildcard() {
        let env: EnvironmentId = "*".into();
        assert!(env.is_wildcard());
        assert!(env.name().is_none());
    }

    #[test]
    fn from_str_handles_well_known_names() {
        for w in WellKnownEnv::all() {
            let env: EnvironmentId = w.as_str().into();
            assert_eq!(env.name(), Some(w.as_str()));
            assert!(!env.is_wildcard());
        }
    }

    #[test]
    fn from_str_handles_custom_env_names() {
        let env: EnvironmentId = "x-cambriantech-bevy-rig".into();
        assert_eq!(env.name(), Some("x-cambriantech-bevy-rig"));
    }

    #[test]
    fn well_known_env_canonical_names() {
        assert_eq!(WellKnownEnv::Web.as_str(), "web");
        assert_eq!(WellKnownEnv::Tty.as_str(), "tty");
        assert_eq!(WellKnownEnv::Cli.as_str(), "cli");
        assert_eq!(WellKnownEnv::Vr.as_str(), "vr");
        assert_eq!(WellKnownEnv::Ar.as_str(), "ar");
        assert_eq!(WellKnownEnv::Headless.as_str(), "headless");
    }

    #[test]
    fn well_known_to_environment_id_round_trips() {
        for w in WellKnownEnv::all() {
            let env = EnvironmentId::well_known(*w);
            assert_eq!(env.name(), Some(w.as_str()));
            // Round-trip via Display + From<&str>
            assert_eq!(EnvironmentId::from(env.to_string().as_str()), env);
        }
    }

    #[test]
    fn well_known_env_is_hashable_for_registry_use() {
        use std::collections::HashSet;
        let mut s = HashSet::new();
        s.insert(WellKnownEnv::Web);
        s.insert(WellKnownEnv::Vr);
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn environment_id_is_hashable_for_registry_use() {
        use std::collections::HashSet;
        let mut s = HashSet::new();
        s.insert(EnvironmentId::from("web"));
        s.insert(EnvironmentId::from("vr"));
        s.insert(EnvironmentId::from("*"));
        assert_eq!(s.len(), 3);
    }
}
