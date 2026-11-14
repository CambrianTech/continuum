//! Structured failures at the declaration/physical-store boundary.
//! Callers classify these variants; diagnostic text is never a control protocol.
#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("{table}.{column}: numeric conversion requires canonical signed integer text or NULL; data unchanged")]
    NonCanonicalIntegerText { table: super::names::TableName, column: super::names::ColumnName },
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Codec(#[from] serde_json::Error),
    #[error("{collection}.{field}: declared type {previous:?} -> {requested:?} requires an explicit conversion")]
    DeclaredTypeConversion {
        collection: super::names::CollectionName,
        field: super::names::FieldName,
        previous: super::types::FieldType,
        requested: super::types::FieldType,
    },
    #[error("{table}.{column}: physical type {previous} -> {requested} requires an explicit conversion")]
    PhysicalTypeConversion {
        table: super::names::TableName,
        column: super::names::ColumnName,
        previous: super::names::PhysicalType,
        requested: super::names::PhysicalType,
    },
    #[error("{collection}: declaration gap: {fault}")]
    DeclarationGap { collection: super::names::CollectionName, fault: GapFault },
    #[error("{collection}: {difference}")]
    Difference { collection: super::names::CollectionName, difference: SchemaDiff },
}

/// What a declaration cannot carry, or what proving it failed on.
#[derive(Debug, thiserror::Error)]
pub enum GapFault {
    #[error("generated columns require expression proof")]
    GeneratedColumn,
    #[error("expression indexes require expression proof")]
    ExpressionIndex,
    #[error("the declaration does not compile: {0}")]
    Compile(#[source] Box<super::StorageError>),
    #[error("schema receipt identity mismatch")]
    ReceiptIdentity,
    #[error("physical schema receipt is not an array")]
    ReceiptNotArray,
    #[error("physical schema receipt row is not an object")]
    ReceiptRowNotObject,
    #[error("added field requires a declared value or constraint rebuild")]
    AddedFieldNeedsValue,
    #[error("added required column has no declared default")]
    AddedColumnNoDefault,
    #[error("the evolution copy query does not build: {0}")]
    CopyQuery(#[source] sea_orm::sea_query::error::Error),
    #[error("foreign-key violation after evolution")]
    ForeignKeyViolation,
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
    pub(super) fn gap(collection: &str, fault: GapFault) -> Self {
        Self::DeclarationGap { collection: collection.into(), fault }
    }

    pub(super) fn difference(collection: &str, difference: SchemaDiff) -> Self {
        Self::Difference { collection: collection.into(), difference }
    }
}
