//! A cursor page of a room's chat store, keyed by `(timestamp, id)`.
//!
//! A cursor that filters on the timestamp alone loses messages: rows that share
//! the anchor's millisecond fall on neither side of `$lt`/`$gt`, so a page that
//! ends inside a tie drops the rest of that tie for good. The M5's store held 635
//! such tie groups (1,636 rows, 2026-10-26), each one a hole a scroll-back could
//! open. Keyed by `(timestamp, id)`, every row has exactly one place in the order,
//! so consecutive pages neither overlap nor skip.
use super::*;

/// Which way a cursor pages from its anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PageDirection {
    /// Rows strictly older than the anchor: scroll-back.
    Older,
    /// Rows strictly newer than the anchor: the live poll's catch-up.
    Newer,
}

impl PageDirection {
    fn strictly(self) -> &'static str {
        match self {
            PageDirection::Older => "$lt",
            PageDirection::Newer => "$gt",
        }
    }

    /// Nearest-to-the-anchor first, so `limit` keeps the rows adjacent to it.
    fn sort(self) -> &'static str {
        match self {
            PageDirection::Older => "desc",
            PageDirection::Newer => "asc",
        }
    }
}

impl ChatModule {
    /// The `limit` rows adjacent to `anchor` in `direction`, nearest first: the
    /// anchor's own tie (same timestamp, id past the anchor's), then the rows past
    /// its timestamp.
    pub(super) async fn keyset_page(
        &self,
        room_id: Option<Uuid>,
        anchor: Uuid,
        direction: PageDirection,
        limit: usize,
    ) -> Result<Vec<Value>, String> {
        let anchor_timestamp = self.anchor_timestamp(anchor).await?;
        let strictly = direction.strictly();
        let sort = direction.sort();

        let mut tie = room_filter(room_id);
        tie.insert("timestamp".to_string(), json!({ "$eq": anchor_timestamp }));
        tie.insert("id".to_string(), operator(strictly, json!(anchor.to_string())));
        let mut rows = self
            .query_rows(CHAT_MESSAGES_COLLECTION, Value::Object(tie), Some(json!([{ "field": "id", "direction": sort }])), Some(limit))
            .await?;

        if rows.len() < limit {
            let mut past = room_filter(room_id);
            past.insert("timestamp".to_string(), operator(strictly, json!(anchor_timestamp)));
            let order = json!([
                { "field": "timestamp", "direction": sort },
                { "field": "id", "direction": sort },
            ]);
            rows.extend(self.query_rows(CHAT_MESSAGES_COLLECTION, Value::Object(past), Some(order), Some(limit - rows.len())).await?);
        }
        Ok(rows)
    }

    /// The newest `limit` rows (no cursor).
    pub(super) async fn newest_page(&self, room_id: Option<Uuid>, limit: usize) -> Result<Vec<Value>, String> {
        let order = json!([
            { "field": "timestamp", "direction": "desc" },
            { "field": "id", "direction": "desc" },
        ]);
        self.query_rows(CHAT_MESSAGES_COLLECTION, Value::Object(room_filter(room_id)), Some(order), Some(limit)).await
    }
}

fn operator(op: &str, operand: Value) -> Value {
    Value::Object(serde_json::Map::from_iter([(op.to_string(), operand)]))
}

fn room_filter(room_id: Option<Uuid>) -> serde_json::Map<String, Value> {
    let mut filter = serde_json::Map::new();
    if let Some(room_id) = room_id {
        filter.insert("roomId".to_string(), json!({ "$eq": room_id.to_string() }));
    }
    filter
}

/// Chronological display order, ties broken by id: the same total order the
/// cursors page by.
pub(super) fn chronological(rows: &mut [Value]) {
    let key = |row: &Value| {
        let field = |name: &str| row.get(name).and_then(Value::as_str).map(str::to_owned);
        (field("timestamp"), field("id"))
    };
    rows.sort_by_cached_key(key);
}
