//! Coalesced native changes from the same pinned snapshot as their payload bytes.
//! This is reconciliation of native fields, not an immutable event log or GPU receipt.
use super::{
    binary::{descriptor_column, native_identity, BinaryDescriptor},
    types::FieldType,
    BinarySnapshot,
};
use sea_orm::sea_query::{Alias, Expr, JoinType, Order, Query};
use sea_orm::{ConnectionTrait, DbBackend};
use std::{num::NonZeroUsize, ops::Range};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeDeltaCursor {
    pub revision: u64,
    pub id: uuid::Uuid,
}

pub struct NativeDeltaRequest<'a> {
    pub field: &'a str,
    /// Last fully applied native snapshot revision; zero means no previous state.
    pub since: u64,
    /// Continue only on the same snapshot with unchanged field and since.
    /// This key is not an independently authenticated/bound completion receipt.
    pub after: Option<NativeDeltaCursor>,
    pub limit: NonZeroUsize,
}

#[derive(Debug, PartialEq)]
pub enum NativeDeltaValue {
    /// Absolute range in the caller's byte buffer; bytes retain their producer form.
    Upsert {
        descriptor: BinaryDescriptor,
        bytes: Range<usize>,
    },
    /// The row was deleted, or this optional native field is absent.
    Remove,
}

#[derive(Debug, PartialEq)]
pub struct NativeDelta {
    pub cursor: NativeDeltaCursor,
    pub value: NativeDeltaValue,
}

pub struct NativeDeltaOutput<'a> {
    pub bytes: &'a mut Vec<u8>,
    pub changes: &'a mut Vec<NativeDelta>,
}

#[derive(Debug, thiserror::Error)]
pub enum NativeDeltaError {
    #[error("native delta revision or cursor is outside this snapshot interval")]
    Interval,
    #[error("native delta requires a declared binary field")]
    Field,
    #[error("native delta row has inconsistent payload custody")]
    Custody,
    #[error("native delta provider: {0}")]
    Provider(String),
    #[error("native delta descriptor or identity: {0}")]
    Metadata(String),
    #[error("native delta output exceeds available address space")]
    Capacity,
}

impl BinarySnapshot {
    /// Newest revision first, then descending UUID within a shared revision.
    /// None means this interval has no further page, not that a consumer adopted it.
    /// All rows validate before either caller buffer changes; cancellation is atomic.
    pub async fn changes_into(
        &self,
        request: &NativeDeltaRequest<'_>,
        output: NativeDeltaOutput<'_>,
    ) -> Result<Option<NativeDeltaCursor>, NativeDeltaError> {
        use sea_orm::sqlx::Row;
        let high = self.revision();
        if request.since > high
            || request
                .after
                .is_some_and(|cursor| cursor.revision > high || cursor.revision <= request.since)
        {
            return Err(NativeDeltaError::Interval);
        }
        if !self
            .schema
            .fields
            .iter()
            .any(|field| field.name == request.field && field.field_type == FieldType::Binary)
        {
            return Err(NativeDeltaError::Field);
        }
        let column = super::relational_query::column(&self.schema, request.field)
            .map_err(NativeDeltaError::Metadata)?;
        let ledger = Alias::new("r");
        let data = Alias::new("d");
        let revision = (ledger.clone(), Alias::new("revision"));
        let id = (ledger.clone(), Alias::new("id"));
        let mut query = Query::select();
        query
            .from_as(super::relational_native_revision::Entity, ledger.clone())
            .expr_as(Expr::col(id.clone()), Alias::new("id"))
            .expr_as(Expr::col(revision.clone()), Alias::new("revision"))
            .expr_as(
                Expr::col((ledger.clone(), Alias::new("deleted"))),
                Alias::new("deleted"),
            )
            .expr_as(
                Expr::col((data.clone(), Alias::new("id"))),
                Alias::new("live_id"),
            )
            .expr_as(Expr::col((data.clone(), column)), Alias::new("payload"))
            .expr_as(
                Expr::col((data.clone(), Alias::new(descriptor_column(request.field)))),
                Alias::new("descriptor"),
            )
            .join_as(
                JoinType::LeftJoin,
                Alias::new(super::adapter::naming::to_table_name(
                    &self.schema.collection,
                )),
                data.clone(),
                Expr::col(id.clone()).equals((data, Alias::new("id"))),
            )
            .and_where(
                Expr::col((ledger, Alias::new("collection"))).eq(self.schema.collection.as_str()),
            )
            .and_where(
                Expr::col(revision.clone())
                    .gt(i64::try_from(request.since).map_err(|_| NativeDeltaError::Interval)?),
            )
            .and_where(
                Expr::col(revision.clone())
                    .lte(i64::try_from(high).map_err(|_| NativeDeltaError::Interval)?),
            )
            .order_by(revision.clone(), Order::Desc)
            .order_by(id.clone(), Order::Desc)
            .limit(u64::try_from(request.limit.get()).map_err(|_| NativeDeltaError::Capacity)?);
        if let Some(after) = request.after {
            let at = i64::try_from(after.revision).map_err(|_| NativeDeltaError::Interval)?;
            query.and_where(
                Expr::col(revision.clone()).lt(at).or(Expr::col(revision)
                    .eq(at)
                    .and(Expr::col(id).lt(after.id.to_string()))),
            );
        }
        let rows = self
            .transaction
            .query_all(DbBackend::Sqlite.build(&query))
            .await
            .map_err(|error| NativeDeltaError::Provider(error.to_string()))?;
        let mut staged = Vec::with_capacity(rows.len());
        let mut end = output.bytes.len();
        for row in &rows {
            let row = row.try_as_sqlite_row().ok_or(NativeDeltaError::Custody)?;
            let id: &str = row
                .try_get("id")
                .map_err(|error| NativeDeltaError::Provider(error.to_string()))?;
            let revision: i64 = row
                .try_get("revision")
                .map_err(|error| NativeDeltaError::Provider(error.to_string()))?;
            let deleted: bool = row
                .try_get("deleted")
                .map_err(|error| NativeDeltaError::Provider(error.to_string()))?;
            let live: Option<&str> = row
                .try_get("live_id")
                .map_err(|error| NativeDeltaError::Provider(error.to_string()))?;
            let payload: Option<&[u8]> = row
                .try_get("payload")
                .map_err(|error| NativeDeltaError::Provider(error.to_string()))?;
            let header: Option<&[u8]> = row
                .try_get("descriptor")
                .map_err(|error| NativeDeltaError::Provider(error.to_string()))?;
            let cursor = NativeDeltaCursor {
                revision: u64::try_from(revision).map_err(|_| NativeDeltaError::Interval)?,
                id: native_identity(id).map_err(NativeDeltaError::Metadata)?,
            };
            let (value, bytes) = match (deleted, live, payload, header) {
                (true, _, None, None) | (false, Some(_), None, None) => {
                    (NativeDeltaValue::Remove, None)
                }
                (false, Some(_), Some(bytes), Some(header)) => {
                    let descriptor =
                        BinaryDescriptor::decode(header).map_err(NativeDeltaError::Metadata)?;
                    if descriptor.byte_len().map_err(NativeDeltaError::Metadata)? != bytes.len() {
                        return Err(NativeDeltaError::Custody);
                    }
                    let start = end;
                    end = end
                        .checked_add(bytes.len())
                        .ok_or(NativeDeltaError::Capacity)?;
                    (
                        NativeDeltaValue::Upsert {
                            descriptor,
                            bytes: start..end,
                        },
                        Some(bytes),
                    )
                }
                _ => return Err(NativeDeltaError::Custody),
            };
            staged.push((NativeDelta { cursor, value }, bytes));
        }
        let next = if staged.len() == request.limit.get() {
            staged.last().map(|(change, _)| change.cursor)
        } else {
            None
        };
        output
            .bytes
            .try_reserve(end - output.bytes.len())
            .map_err(|_| NativeDeltaError::Capacity)?;
        output
            .changes
            .try_reserve(staged.len())
            .map_err(|_| NativeDeltaError::Capacity)?;
        // No await or fallible operation after mutation begins.
        for (change, bytes) in staged {
            if let Some(bytes) = bytes {
                output.bytes.extend_from_slice(bytes);
            }
            output.changes.push(change);
        }
        Ok(next)
    }
}
