//! Shared keyset traversal for domain owners that must reconcile every matching row.
//! Each database request is bounded; projections prevent fetching unrelated payloads.
use continuum_client::{ClientError, Connection, Transport};
use serde::de::DeserializeOwned;
use serde_json::{json, Map, Value};

// Decode the requested projection at the command transport boundary. Collection,
// metadata and count are not inputs to keyset traversal and are not hydrated.
#[derive(serde::Deserialize)]
struct SelectedRecord<E> {
    id: String,
    data: E,
}

#[derive(serde::Deserialize)]
struct SelectedPage<E> {
    items: Vec<SelectedRecord<E>>,
}

/// Which rows of which store `matching` reads: borrowed, since every page reuses them.
pub struct ListSelection<'a> {
    pub handle: &'a str,
    pub collection: &'a str,
    pub filter: &'a Map<String, Value>,
    /// The fields to materialize; `None` reads whole rows.
    pub select: Option<&'a [&'a str]>,
}

pub async fn matching<T: Transport, E: DeserializeOwned>(
    conn: &Connection<T>,
    selection: ListSelection<'_>,
) -> Result<Vec<E>, ClientError> {
    let ListSelection { handle, collection, filter, select } = selection;
    if filter.contains_key("id") {
        return Err(ClientError::Transport("keyset traversal reserves the id predicate; use a bounded identity lookup".into()));
    }
    let size = crate::orm::query::default_page_size();
    let mut after: Option<String> = None;
    let mut output = Vec::new();
    loop {
        let mut page_filter = filter.clone();
        if let Some(id) = &after { page_filter.insert("id".into(), json!({"$gt": id})); }
        let page: SelectedPage<E> = conn.commands().execute("data/list", json!({
            "dbPath": handle, "collection": collection, "filter": page_filter,
            "select": select, "sort": [{"field": "id", "direction": "asc"}], "limit": size,
        })).await?;
        let count = page.items.len();
        if count > size { return Err(ClientError::Transport("storage returned more rows than requested".into())); }
        for record in page.items {
            if after.as_ref().is_some_and(|previous| record.id <= *previous) {
                return Err(ClientError::Transport("storage keyset page did not advance in identity order".into()));
            }
            after = Some(record.id);
            output.push(record.data);
        }
        if count < size { return Ok(output); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_client::mock::MockTransport;

    /// The fixture store's selection of `filter` and `select`.
    fn fixture<'a>(filter: &'a Map<String, Value>, select: Option<&'a [&'a str]>) -> ListSelection<'a> {
        ListSelection { handle: "fixture", collection: "fixture", filter, select }
    }
    #[derive(serde::Deserialize)]
    struct Row { value: usize }
    fn page(rows: std::ops::Range<usize>, total: usize) -> Value {
        let items: Vec<_> = rows.map(|n| json!({
            "id": format!("{n:06}"), "collection": "fixture", "data": {"value": n},
            "metadata": {"createdAt":"now", "updatedAt":"now", "version":1}
        })).collect();
        json!({"items": items, "total": total})
    }
    #[tokio::test]
    async fn deletion_before_the_cursor_does_not_skip_the_next_row() -> Result<(), ClientError> {
        let size = crate::orm::query::default_page_size();
        let mock = MockTransport::new();
        mock.respond_to("data/list", move |params| {
            assert_eq!(params["limit"], size);
            assert_eq!(params["select"], json!(["value"]));
            Ok(page(0..size, size + 1))
        });
        mock.respond_to("data/list", move |params| {
            assert_eq!(params["filter"]["id"]["$gt"], format!("{:06}", size - 1));
            // Every previously delivered row disappeared; OFFSET would skip this row.
            Ok(page(size..size + 1, 1))
        });
        let rows: Vec<Row> = matching(&Connection::new(mock), fixture(&Map::new(), Some(&["value"]))).await?;
        assert_eq!(rows.len(), size + 1);
        assert_eq!(rows.last().map(|row| row.value), Some(size));
        Ok(())
    }
    #[tokio::test]
    async fn repeated_page_is_refused_instead_of_looping_or_returning_partial_success() {
        let size = crate::orm::query::default_page_size();
        let mock = MockTransport::new();
        mock.respond_with("data/list", page(0..size, size * 2));
        mock.respond_with("data/list", page(0..size, size * 2));
        let result = matching::<_, Row>(&Connection::new(mock), fixture(&Map::new(), None)).await;
        assert!(matches!(result, Err(ClientError::Transport(message)) if message.contains("did not advance")));
    }
    #[tokio::test]
    async fn projected_values_keep_exact_integers_and_reject_null_required_fields() -> Result<(), ClientError> {
        #[derive(Debug, serde::Deserialize, PartialEq)]
        struct Projection { sequence: u64, note: Option<String> }
        let mock = MockTransport::new();
        mock.respond_with("data/list", json!({"items":[{
            "id":"a", "data":{"sequence":u64::MAX,"note":null}
        }]}));
        let rows: Vec<Projection> = matching(&Connection::new(mock), fixture(&Map::new(), Some(&["sequence", "note"]))).await?;
        assert_eq!(rows, vec![Projection { sequence: u64::MAX, note: None }]);
        let mock = MockTransport::new();
        mock.respond_with("data/list", json!({"items":[{
            "id":"a", "data":{"sequence":null,"note":null}
        }]}));
        assert!(matching::<_, Projection>(&Connection::new(mock), fixture(&Map::new(), Some(&["sequence", "note"]))).await.is_err());
        Ok(())
    }
}
