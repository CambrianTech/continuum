//! Query Builder - Database-agnostic query construction
//!
//! Provides a fluent API for building queries that adapters translate to native format.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// Sort direction
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../../shared/generated/orm/SortDirection.ts")]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    Desc,
}

/// Comparable value type for query operations
pub type ComparableValue = Value;

/// Query operators for filtering
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/QueryOperator.ts")]
#[serde(rename_all = "camelCase")]
pub enum QueryOperator {
    /// Equal to
    Eq(#[ts(type = "string | number | boolean | null")] ComparableValue),
    /// Not equal to
    Ne(#[ts(type = "string | number | boolean | null")] ComparableValue),
    /// Greater than
    Gt(#[ts(type = "string | number | boolean")] ComparableValue),
    /// Greater than or equal
    Gte(#[ts(type = "string | number | boolean")] ComparableValue),
    /// Less than
    Lt(#[ts(type = "string | number | boolean")] ComparableValue),
    /// Less than or equal
    Lte(#[ts(type = "string | number | boolean")] ComparableValue),
    /// In array
    In(#[ts(type = "Array<string | number | boolean>")] Vec<ComparableValue>),
    /// Not in array
    NotIn(#[ts(type = "Array<string | number | boolean>")] Vec<ComparableValue>),
    /// Field exists
    Exists(bool),
    /// Regex match
    Regex(String),
    /// String contains (case insensitive)
    Contains(String),
    /// Is null
    IsNull,
    /// Is not null
    IsNotNull,
}

/// Field filter - either a direct value or an operator
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/FieldFilter.ts")]
#[serde(untagged)]
pub enum FieldFilter {
    /// Direct value (implies Eq)
    Value(#[ts(type = "string | number | boolean | null")] ComparableValue),
    /// Operator-based filter
    Operator(QueryOperator),
}

/// Sort specification
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/SortSpec.ts")]
#[serde(rename_all = "camelCase")]
pub struct SortSpec {
    pub field: String,
    pub direction: SortDirection,
}

/// Cursor for pagination
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/Cursor.ts")]
#[serde(rename_all = "camelCase")]
pub struct Cursor {
    pub field: String,
    #[ts(type = "string | number | boolean")]
    pub value: ComparableValue,
    pub direction: CursorDirection,
}

/// Cursor direction
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../../shared/generated/orm/CursorDirection.ts")]
#[serde(rename_all = "lowercase")]
pub enum CursorDirection {
    Before,
    After,
}

/// Time range filter
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/TimeRange.ts")]
#[serde(rename_all = "camelCase")]
pub struct TimeRange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
}

/// Join specification for related data loading
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/JoinSpec.ts")]
#[serde(rename_all = "camelCase")]
pub struct JoinSpec {
    /// Collection to join with
    pub collection: String,
    /// Alias for the joined data in results
    pub alias: String,
    /// Field in the primary collection
    pub local_field: String,
    /// Field in the joined collection
    pub foreign_field: String,
    /// Join type
    pub join_type: JoinType,
    /// Fields to select from joined collection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub select: Option<Vec<String>>,
}

/// Join type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../../shared/generated/orm/JoinType.ts")]
#[serde(rename_all = "lowercase")]
pub enum JoinType {
    Left,
    Inner,
}

/// Storage query - the universal query format
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../shared/generated/orm/StorageQuery.ts")]
#[serde(rename_all = "camelCase")]
pub struct StorageQuery {
    pub collection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<std::collections::HashMap<String, FieldFilter>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Vec<SortSpec>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<Cursor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_range: Option<TimeRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joins: Option<Vec<JoinSpec>>,
}

/// Fluent query builder
pub struct QueryBuilder {
    query: StorageQuery,
}

impl QueryBuilder {
    /// Create a new query builder for a collection
    pub fn new(collection: impl Into<String>) -> Self {
        Self {
            query: StorageQuery {
                collection: collection.into(),
                ..Default::default()
            },
        }
    }

    /// Add an equality filter
    pub fn filter_eq(mut self, field: impl Into<String>, value: impl Into<Value>) -> Self {
        let filter = self.query.filter.get_or_insert_with(Default::default);
        filter.insert(field.into(), FieldFilter::Value(value.into()));
        self
    }

    /// Add an operator-based filter
    pub fn filter(mut self, field: impl Into<String>, op: QueryOperator) -> Self {
        let filter = self.query.filter.get_or_insert_with(Default::default);
        filter.insert(field.into(), FieldFilter::Operator(op));
        self
    }

    /// Add sort specification
    pub fn sort(mut self, field: impl Into<String>, direction: SortDirection) -> Self {
        let sorts = self.query.sort.get_or_insert_with(Vec::new);
        sorts.push(SortSpec {
            field: field.into(),
            direction,
        });
        self
    }

    /// Sort ascending
    pub fn sort_asc(self, field: impl Into<String>) -> Self {
        self.sort(field, SortDirection::Asc)
    }

    /// Sort descending
    pub fn sort_desc(self, field: impl Into<String>) -> Self {
        self.sort(field, SortDirection::Desc)
    }

    /// Set limit
    pub fn limit(mut self, limit: usize) -> Self {
        self.query.limit = Some(limit);
        self
    }

    /// Set offset
    pub fn offset(mut self, offset: usize) -> Self {
        self.query.offset = Some(offset);
        self
    }

    /// Add a join
    pub fn join(mut self, spec: JoinSpec) -> Self {
        let joins = self.query.joins.get_or_insert_with(Vec::new);
        joins.push(spec);
        self
    }

    /// Build the query
    pub fn build(self) -> StorageQuery {
        self.query
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_builder_basic() {
        let query = QueryBuilder::new("users")
            .filter_eq("name", "Joel")
            .sort_desc("createdAt")
            .limit(10)
            .build();

        assert_eq!(query.collection, "users");
        assert_eq!(query.limit, Some(10));
        assert!(query.filter.is_some());
        assert!(query.sort.is_some());
    }

    #[test]
    fn test_query_builder_operators() {
        let query = QueryBuilder::new("messages")
            .filter("timestamp", QueryOperator::Gte(Value::from("2024-01-01")))
            .filter("priority", QueryOperator::In(vec![Value::from(1), Value::from(2)]))
            .build();

        let filter = query.filter.unwrap();
        assert!(filter.contains_key("timestamp"));
        assert!(filter.contains_key("priority"));
    }
}
