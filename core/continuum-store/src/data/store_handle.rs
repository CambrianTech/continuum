//! Which store a data or vector command addresses.

/// A store a command addresses by name: the shared main store unless the caller names another.
/// The default is a value of this type, never an `Option` that every command resolves on its own
/// (18 commands each carried `handle.as_deref().unwrap_or("main")` before this owner).
/// On the wire the field is `dbPath` and may be omitted; in Rust it is always a store.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(transparent)]
#[ts(export, export_to = "../../../protocol/typescript/data/StoreHandle.ts")]
pub struct StoreHandle(String);

impl StoreHandle {
    /// The shared store every citizen reads and writes.
    pub const MAIN: &'static str = "main";

    /// The store's name, as the data layer resolves it.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The shared main store: what an omitted `dbPath` means, so it is never written back out.
    pub fn is_main(&self) -> bool {
        self.0 == Self::MAIN
    }
}

impl Default for StoreHandle {
    fn default() -> Self {
        Self(Self::MAIN.to_owned())
    }
}
