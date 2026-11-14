//! Structured failures at the declaration/physical-store boundary.
//! Callers classify these variants; diagnostic text is never a control protocol.
#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Codec(#[from] serde_json::Error),
    #[error("{collection}: declaration gap: {detail}")]
    DeclarationGap { collection: String, detail: String },
    #[error("{collection}: {difference}")]
    Difference { collection: String, difference: SchemaDiff },
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum SchemaDiff {
    #[error("declared or physical schema changed")]
    Declaration,
    #[error("physical schema differs from its current declaration; admission refused")]
    CurrentPhysical,
    #[error("physical schema changed since its receipt")]
    RecordedPhysical,
}

impl SchemaError {
    pub(super) fn gap(collection: &str, detail: impl Into<String>) -> Self {
        Self::DeclarationGap { collection: collection.into(), detail: detail.into() }
    }

    pub(super) fn difference(collection: &str, difference: SchemaDiff) -> Self {
        Self::Difference { collection: collection.into(), difference }
    }
}
