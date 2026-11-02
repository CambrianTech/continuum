//! A mind's drain over the store's arrivals (PERCEPTION-FROM-THE-STORE.md, P2): the next page after
//! her cursor, in commit order, in her rooms, with an explicit gap whenever history she has not read
//! is gone or was never vouched for. Routing reads the typed arrival header only; the payload is the
//! consumer's to open.
//!
//! Fairness across rooms needs no scheduler: her cursor only moves forward through commit order, so a
//! row in a quiet room is reached after at most one page of a busy room's traffic, never starved.
//!
//! Durable acceptance (committing her pending perception, the cursor prefix and the gap receipts
//! together) is the shared consumption owner's (Codex, #5691 773d34a4); this module decides WHAT the
//! next page is, never whether it was accepted.

use airc_core::{PeerId, RoomId};
use serde_json::{json, Value};

use super::arrival::Arrival;
use super::chat_data_handle::{ChatModule, CHAT_DATA_HANDLE};
use super::chat_module::extract_records_as_data;

/// Why history between two sequences cannot be perceived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapReason {
    /// Pruned by the store's retention before she read it.
    Retention,
    /// Imported before the store vouched for completeness (a migrated watermark): lines may be
    /// missing, so she is told, once, before her first rows.
    UnvouchedImport,
}

/// Where a mind stands in the arrival order: everything through `through` has been accepted (or
/// receipted as a gap); `unvouched_receipted` once she has been told about an unvouched import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrainCursor {
    pub through: u64,
    pub unvouched_receipted: bool,
}

/// The next thing a mind perceives from the store.
#[derive(Debug, Clone, PartialEq)]
pub enum Drained {
    /// Rows to perceive, in commit order, and the sequence the page scanned through (her own rows
    /// are skipped but still advance it, so they are never re-scanned).
    Rows {
        rows: Vec<Arrival>,
        scanned_through: u64,
    },
    /// History she cannot perceive, said once; accepting it moves her cursor to `through`.
    Gap {
        from: u64,
        through: u64,
        reason: GapReason,
    },
    /// Nothing after her cursor in her rooms.
    Empty,
}

/// What the store says around a mind's cursor: the oldest retained sequence (`None` for an empty
/// collection) and whether her rooms' imports are unvouched before the store owned the watermark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreFacts {
    pub retained_floor: Option<u64>,
    pub unvouched_import: bool,
}

/// Decide the next drain outcome from her cursor, the store's facts and the page of arrivals after
/// her cursor (in commit order, already restricted to her rooms). Pure: the store query and the
/// acceptance are around it.
pub fn next(cursor: DrainCursor, me: PeerId, facts: StoreFacts, page: Vec<Arrival>) -> Drained {
    if facts.unvouched_import && !cursor.unvouched_receipted {
        return Drained::Gap {
            from: 0,
            through: cursor.through,
            reason: GapReason::UnvouchedImport,
        };
    }
    if let Some(floor) = facts.retained_floor {
        if cursor.through + 1 < floor {
            return Drained::Gap {
                from: cursor.through + 1,
                through: floor - 1,
                reason: GapReason::Retention,
            };
        }
    }
    let Some(scanned_through) = page.iter().map(|a| a.arrival_seq).max() else {
        return Drained::Empty;
    };
    let rows = page.into_iter().filter(|a| a.author != me).collect();
    Drained::Rows {
        rows,
        scanned_through,
    }
}

impl ChatModule {
    /// The page after `cursor` in `rooms`, at most `intake` rows (her intake, the consumer's to size),
    /// and the store facts the drain decision needs, from the store's own indexes.
    pub async fn drain_page(
        &self,
        cursor: DrainCursor,
        rooms: &[RoomId],
        intake: usize,
    ) -> Result<(StoreFacts, Vec<Arrival>), String> {
        let executor = self.executor()?;
        let collection = <Arrival as crate::orm::OrmEntity>::COLLECTION;
        let rooms_typed = rooms;
        let rooms: Vec<String> = rooms.iter().map(|r| r.as_uuid().to_string()).collect();
        let page = json!({
            "dbPath": CHAT_DATA_HANDLE,
            "collection": collection,
            "filter": { "arrivalSeq": { "$gt": cursor.through }, "room": { "$in": rooms } },
            "sort": [{ "field": "arrivalSeq", "direction": "asc" }],
            "limit": intake,
        });
        let floor = json!({
            "dbPath": CHAT_DATA_HANDLE,
            "collection": collection,
            "filter": {},
            "sort": [{ "field": "arrivalSeq", "direction": "asc" }],
            "limit": 1,
        });
        let rows = executor
            .execute_json("data/query", page)
            .await
            .map_err(|e| format!("drain: page query failed: {e}"))?;
        let oldest = executor
            .execute_json("data/query", floor)
            .await
            .map_err(|e| format!("drain: floor query failed: {e}"))?;
        let arrivals = decode(extract_records_as_data(&rows))?;
        let retained_floor = decode(extract_records_as_data(&oldest))?
            .first()
            .map(|a| a.arrival_seq);
        let mut unvouched_import = false;
        for room in rooms_typed {
            if let Some(imported) = self.import_watermark(*room).await? {
                unvouched_import |= matches!(
                    imported.history,
                    super::import_watermark::ImportHistory::UnvouchedThrough { .. }
                );
            }
        }
        Ok((
            StoreFacts {
                retained_floor,
                unvouched_import,
            },
            arrivals,
        ))
    }
}

fn decode(rows: Vec<Value>) -> Result<Vec<Arrival>, String> {
    let decode_row = |row: Value| serde_json::from_value::<Arrival>(row); // storage boundary: the adapter's row format
    rows.into_iter().map(|row| decode_row(row).map_err(|e| format!("drain: malformed arrival row: {e}"))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::arrival::ArrivalKind;
    use uuid::Uuid;

    fn arrival(seq: u64, room: RoomId, author: PeerId) -> Arrival {
        let mut a = Arrival::new(Uuid::new_v4(), room, author, ArrivalKind::Message, None);
        a.arrival_seq = seq;
        a
    }

    const FRESH: DrainCursor = DrainCursor {
        through: 0,
        unvouched_receipted: true,
    };
    const WHOLE: StoreFacts = StoreFacts {
        retained_floor: Some(1),
        unvouched_import: false,
    };

    // what this catches: her own lines delivered back to her, or skipped without advancing the
    // cursor (so the same page is scanned forever): skipped, and the scan covers them.
    #[test]
    fn her_own_rows_are_skipped_but_advance_the_scan() {
        let (me, peer, room) = (PeerId::new(), PeerId::new(), RoomId::new());
        let page = vec![
            arrival(1, room, peer),
            arrival(2, room, me),
            arrival(3, room, peer),
        ];
        let Drained::Rows {
            rows,
            scanned_through,
        } = next(FRESH, me, WHOLE, page)
        else {
            panic!("rows expected");
        };
        assert_eq!(
            rows.iter().map(|a| a.arrival_seq).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(
            scanned_through, 3,
            "her own row 2 is scanned through, never re-read"
        );
        assert_eq!(
            next(
                DrainCursor {
                    through: 3,
                    ..FRESH
                },
                me,
                WHOLE,
                vec![]
            ),
            Drained::Empty
        );
    }

    // what this catches: pruned history read as silence (her cursor below the retained floor
    // silently jumping), and an unvouched import never told: both are explicit gaps, unvouched first.
    #[test]
    fn lost_or_unvouched_history_is_an_explicit_gap() {
        let (me, peer, room) = (PeerId::new(), PeerId::new(), RoomId::new());
        let pruned = StoreFacts {
            retained_floor: Some(10),
            unvouched_import: false,
        };
        assert_eq!(
            next(
                DrainCursor {
                    through: 4,
                    ..FRESH
                },
                me,
                pruned,
                vec![arrival(10, room, peer)]
            ),
            Drained::Gap {
                from: 5,
                through: 9,
                reason: GapReason::Retention
            }
        );
        let unvouched = StoreFacts {
            retained_floor: Some(1),
            unvouched_import: true,
        };
        let untold = DrainCursor {
            through: 0,
            unvouched_receipted: false,
        };
        assert_eq!(
            next(untold, me, unvouched, vec![arrival(1, room, peer)]),
            Drained::Gap {
                from: 0,
                through: 0,
                reason: GapReason::UnvouchedImport
            }
        );
        let told = DrainCursor {
            unvouched_receipted: true,
            ..untold
        };
        assert!(
            matches!(
                next(told, me, unvouched, vec![arrival(1, room, peer)]),
                Drained::Rows { .. }
            ),
            "told once, then her rows"
        );
    }
}
