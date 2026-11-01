//! Core's half of the store's boot seam (card 2709ec6d). The store cannot name the persona
//! substrate (it sits below the mind), so the substrate entities it must register before the first
//! `ensure-schema` are supplied here, by the layer that knows them, and every core caller builds the
//! data module through [`data_module`].

use crate::modules::data::DataModule;

/// Register the substrate's Rust-authored ORM entities into the global registry. Idempotent;
/// returns how many collections the registry resolves afterwards. A conflict refuses boot.
pub fn wire_substrate_orm_entities() -> Result<usize, String> {
    let registry = crate::orm::OrmEntityRegistry::global();
    crate::persona::register_substrate_orm_entities(registry)
        .map_err(|e| format!("substrate ORM entity registration conflict — boot refused: {e}"))?;
    continuum_mind::chat::orm_entities::register_chat_orm_entities(registry)
        .map_err(|e| format!("chat ORM entity registration conflict — boot refused: {e}"))?;
    Ok(substrate_collections()
        .iter()
        .filter(|c| registry.resolve(c).is_some())
        .count())
}

/// Every collection the boot wiring registers: the persona substrate's and the chat store's.
fn substrate_collections() -> Vec<&'static str> {
    crate::persona::SUBSTRATE_ORM_COLLECTIONS
        .iter()
        .chain(continuum_mind::chat::orm_entities::CHAT_ORM_COLLECTIONS)
        .copied()
        .collect()
}

/// The data module as this core boots it: the store's module with the substrate's entities.
pub fn data_module() -> DataModule {
    DataModule::new(wire_substrate_orm_entities)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// what this catches: the substrate's own collections unknown to the ORM at
    /// runtime — `register_substrate_orm_entities` had no production caller for
    /// weeks; `staged_credit` was refused by ensure-schema on every node and the
    /// learning flywheel never staged a credit. Boot must wire it, and the wiring
    /// must make every substrate collection resolvable on the GLOBAL registry.
    #[test]
    fn boot_wiring_makes_every_substrate_collection_resolvable_on_the_global_registry() {
        let n = wire_substrate_orm_entities().expect("no conflict on a fresh or repeated wiring"); // JUSTIFIED: the invariant under test
        assert_eq!(n, substrate_collections().len(), "every substrate collection resolves");
        for c in substrate_collections() {
            assert!(crate::orm::OrmEntityRegistry::global().resolve(c).is_some(), "{c} must resolve");
        }
        assert!(crate::orm::OrmEntityRegistry::global().resolve("staged_credit").is_some());
    }
}
