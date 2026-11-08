//! `persona/catalog` — return the raw persona catalog (for UI display).
//!
//! Stateless: the catalog is data on disk loaded by [`crate::persona::load_catalog`];
//! no module state is captured, so this is a unit-struct command that
//! auto-registers onto the ONE registry.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::persona::{load_catalog, PersonaCatalogEntry};

/// Params for `persona/catalog` — none (the whole catalog is returned).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaCatalogParams.ts"
)]
pub struct PersonaCatalogParams {}

/// Result for `persona/catalog` — the catalog entries. A named wrapper (the typed
/// registry requires a named Result type; a bare `Vec` is an inline array with no
/// importable TS dependency).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/PersonaCatalogResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct PersonaCatalogResult {
    /// Every persona definition in the catalog.
    pub entries: Vec<PersonaCatalogEntry>,
}

crate::action_command! {
    /// Return the raw persona catalog — every persona definition (data, not code):
    /// id, display name, provider, type, model preferences, and profile fields. The
    /// UI and seed path read this to render/seed the available citizens. Read-only.
    pub struct PersonaCatalog;
    name: "persona/catalog",
    access: Privileged,
    params: PersonaCatalogParams,
    output: PersonaCatalogResult,
    run(_this, _ctx, _p) => {
        Ok(PersonaCatalogResult { entries: load_catalog() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: name/access wiring — the catalog is an owner/UI read
    // surface, Privileged (not a persona toolbelt action).
    #[test]
    fn name_and_access_wired() {
        assert_eq!(PersonaCatalog::NAME, "persona/catalog");
        assert!(matches!(
            PersonaCatalog::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: the body loads the live catalog and returns a non-empty
    // list of entries — the seed/UI surface still reaches the on-disk catalog.
    #[tokio::test]
    async fn returns_the_loaded_catalog() {
        let out = PersonaCatalog
            .run(&Ctx::default(), PersonaCatalogParams::default())
            .await
            .expect("catalog load must succeed");
        assert!(
            !out.entries.is_empty(),
            "the catalog ships with persona definitions"
        );
    }
}
