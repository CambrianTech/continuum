#![cfg(test)]
use super::*;

/// A `data/query` responder over rows held in memory: it evaluates the `$eq` /
/// `$lt` / `$gt` filters, the multi-key sort and the limit the chat module sends,
/// so a test reads the pages a real store would return rather than asserting the
/// query's shape.
fn store_over(rows: Vec<Value>) -> StubDataModule {
    StubDataModule::query_only(move |params| {
        let filter = params["filter"].as_object().cloned().unwrap_or_default();
        let matches = |row: &Value| {
            filter.iter().all(|(field, ops)| {
                let have = row[field].as_str().unwrap_or_default();
                ops.as_object().into_iter().flatten().all(|(op, want)| {
                    let want = want.as_str().unwrap_or_default();
                    match op.as_str() {
                        "$eq" => have == want,
                        "$lt" => have < want,
                        "$gt" => have > want,
                        other => panic!("store stub: unhandled operator {other}"),
                    }
                })
            })
        };
        let mut hits: Vec<Value> = rows.iter().filter(|r| matches(r)).cloned().collect();
        let sort = params["sort"].as_array().cloned().unwrap_or_default();
        hits.sort_by(|a, b| {
            sort.iter()
                .map(|key| {
                    let field = key["field"].as_str().unwrap_or_default();
                    let order = a[field].as_str().cmp(&b[field].as_str());
                    if key["direction"] == "desc" { order.reverse() } else { order }
                })
                .find(|o| o.is_ne())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if let Some(limit) = params["limit"].as_u64() {
            hits.truncate(limit as usize);
        }
        let data: Vec<Value> = hits.into_iter().map(|row| json!({ "id": row["id"], "data": row })).collect();
        json!({ "success": true, "data": data })
    })
}

/// 23 rows in one room where most share a millisecond with a neighbour (runs of
/// 1-4 rows per timestamp, as the M5's store holds), plus another room's rows
/// interleaved in time that no page may return.
fn tied_room(room: Uuid, other: Uuid) -> (Vec<Value>, Vec<Value>) {
    let mut mine = Vec::new();
    let mut all = Vec::new();
    for i in 0..23u32 {
        let stamp = format!("2026-10-26T12:00:{:02}.000Z", i / 3 + (i % 5) / 4);
        let row = json!({ "id": Uuid::new_v4().to_string(), "roomId": room.to_string(), "timestamp": stamp });
        mine.push(row.clone());
        all.push(row);
        all.push(json!({ "id": Uuid::new_v4().to_string(), "roomId": other.to_string(), "timestamp": stamp }));
    }
    let key = |r: &Value| (r["timestamp"].as_str().map(str::to_owned), r["id"].as_str().map(str::to_owned));
    mine.sort_by_key(key);
    (mine, all)
}

fn ids(rows: &[Value]) -> Vec<String> {
    rows.iter().map(|r| r["id"].as_str().unwrap_or_default().to_string()).collect()
}

// what this catches: scroll-back that skips, repeats or never ends. Starting from the
// newest page and passing the oldest id held each time, the pages must walk the whole
// room exactly once, each strictly older than its anchor, ending on a short page; a
// cursor on the timestamp alone dropped the rest of a same-millisecond tie at every
// page edge, and a cursor the reader ignored re-read the newest page forever.
#[tokio::test]
pub(super) async fn scroll_back_pages_walk_the_room_once_from_newest_to_first() {
    let (room, other) = (Uuid::new_v4(), Uuid::new_v4());
    let (expected, all) = tied_room(room, other);
    let chat = chat_with_stubs(vec![Arc::new(store_over(all))]);
    let limit = 5;

    let mut held: Vec<Value> = Vec::new();
    let mut before = None;
    loop {
        let page = chat
            .poll(ChatPollParams { room_id: Some(room), before_message_id: before, limit: Some(limit), ..Default::default() })
            .await
            .expect("a page of a room that exists");
        if let Some(oldest) = held.first() {
            let anchor = (oldest["timestamp"].as_str(), oldest["id"].as_str());
            for row in &page.messages {
                assert!((row["timestamp"].as_str(), row["id"].as_str()) < anchor, "page row not older than its anchor");
            }
        }
        let mut next = page.messages.clone();
        next.extend(held);
        held = next;
        if page.count < limit {
            break;
        }
        before = held.first().and_then(|r| r["id"].as_str()).map(|id| Uuid::parse_str(id).expect("stored ids are uuids"));
    }
    assert_eq!(ids(&held), ids(&expected), "every row of the room once, in order, and no other room's");
}

// what this catches: the live catch-up (`afterMessageId`) has the same tie edge as
// scroll-back. Walking forward from the room's first row must return every later row
// exactly once.
#[tokio::test]
pub(super) async fn catch_up_pages_walk_the_room_once_from_first_to_newest() {
    let (room, other) = (Uuid::new_v4(), Uuid::new_v4());
    let (expected, all) = tied_room(room, other);
    let chat = chat_with_stubs(vec![Arc::new(store_over(all))]);

    let mut held = vec![expected[0].clone()];
    loop {
        let after = held.last().and_then(|r| r["id"].as_str()).map(|id| Uuid::parse_str(id).expect("stored ids are uuids"));
        let page = chat
            .poll(ChatPollParams { room_id: Some(room), after_message_id: after, limit: Some(4), ..Default::default() })
            .await
            .expect("a page after a row that exists");
        if page.count == 0 {
            break;
        }
        held.extend(page.messages);
    }
    assert_eq!(ids(&held), ids(&expected));
}
