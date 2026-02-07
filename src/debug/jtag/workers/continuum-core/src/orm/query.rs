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
/// Uses MongoDB-style $-prefixed operators to match TypeScript format directly
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/QueryOperator.ts")]
pub enum QueryOperator {
    /// Equal to
    #[serde(rename = "$eq")]
    Eq(#[ts(type = "string | number | boolean | null")] ComparableValue),
    /// Not equal to
    #[serde(rename = "$ne")]
    Ne(#[ts(type = "string | number | boolean | null")] ComparableValue),
    /// Greater than
    #[serde(rename = "$gt")]
    Gt(#[ts(type = "string | number | boolean")] ComparableValue),
    /// Greater than or equal
    #[serde(rename = "$gte")]
    Gte(#[ts(type = "string | number | boolean")] ComparableValue),
    /// Less than
    #[serde(rename = "$lt")]
    Lt(#[ts(type = "string | number | boolean")] ComparableValue),
    /// Less than or equal
    #[serde(rename = "$lte")]
    Lte(#[ts(type = "string | number | boolean")] ComparableValue),
    /// In array
    #[serde(rename = "$in")]
    In(#[ts(type = "Array<string | number | boolean>")] Vec<ComparableValue>),
    /// Not in array
    #[serde(rename = "$nin")]
    NotIn(#[ts(type = "Array<string | number | boolean>")] Vec<ComparableValue>),
    /// Field exists
    #[serde(rename = "$exists")]
    Exists(bool),
    /// Regex match
    #[serde(rename = "$regex")]
    Regex(String),
    /// String contains (case insensitive)
    #[serde(rename = "$contains")]
    Contains(String),
    /// Is null
    #[serde(rename = "$isNull")]
    IsNull,
    /// Is not null
    #[serde(rename = "$isNotNull")]
    IsNotNull,
}

/// Field filter - either a direct value or an operator
/// CRITICAL: Operator MUST come before Value in untagged enum!
/// serde tries variants in order - Operator has more specific pattern ($-prefixed keys)
/// while Value matches ANY JSON value. If Value comes first, Operator never gets tried.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/FieldFilter.ts")]
#[serde(untagged)]
pub enum FieldFilter {
    /// Operator-based filter (must come first for correct parsing)
    Operator(QueryOperator),
    /// Direct value (implies Eq) - fallback for non-operator values
    Value(#[ts(type = "string | number | boolean | null")] ComparableValue),
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
    #[ts(optional)]
        pub start: Option<String>,
    #[ts(optional)]
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
    #[ts(optional)]
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
    #[ts(optional)]
    #[serde(default)]
    pub filter: Option<std::collections::HashMap<String, FieldFilter>>,
    #[ts(optional)]
    #[serde(default)]
    pub sort: Option<Vec<SortSpec>>,
    #[ts(optional)]
    #[serde(default)]
    pub limit: Option<usize>,
    #[ts(optional)]
    #[serde(default)]
    pub offset: Option<usize>,
    #[ts(optional)]
    #[serde(default)]
    pub cursor: Option<Cursor>,
    #[ts(optional)]
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[ts(optional)]
    #[serde(default)]
    pub time_range: Option<TimeRange>,
    #[ts(optional)]
    #[serde(default)]
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
