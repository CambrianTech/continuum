//! "Who is here", REQUIRED: she is present in every room she takes a turn in (her mind room
//! included, where she is the only one), so this grounding is never empty by construction. It is
//! the roster's projection with her own row guaranteed, rendered by the roster's own
//! [`member_unit`], not a fallback: a room's present set always contains the one standing in it.

use std::sync::Arc;

use super::rag_item;
use super::roster::member_unit;
use crate::cognition::rag_budget::{CriticalSource, Grounded, RagContext, RagSource};

/// Who is present in the turn's room, with her always among them.
pub struct WhoIsHere {
    roster: Arc<dyn RagSource>,
    me: uuid::Uuid,
    my_name: String,
}

impl WhoIsHere {
    /// `roster` is the room's shared presence projection; `me` and `my_name` are hers.
    pub fn new(roster: Arc<dyn RagSource>, me: uuid::Uuid, my_name: impl Into<String>) -> Self {
        Self {
            roster,
            me,
            my_name: my_name.into(),
        }
    }
}

#[async_trait::async_trait]
impl CriticalSource for WhoIsHere {
    fn source_id(&self) -> &'static str {
        self.roster.source_id()
    }

    fn expand_command(&self) -> Option<&'static str> {
        self.roster.expand_command()
    }

    async fn deliver(&self, ctx: &RagContext) -> Grounded {
        let delivery = self.roster.deliver(ctx).await;
        let me = self.me.to_string();
        let mut others = delivery.items;
        let mine = match others.iter().position(|item| item.key == me) {
            Some(at) => others.remove(at),
            None => rag_item(
                member_unit(
                    self.me,
                    &self.my_name,
                    continuum_positron::SenderKind::Agent,
                    None,
                    ctx.persona_id,
                ),
                continuum_positron::RosterViewState::KIND,
                <continuum_positron::RosterViewState as super::RagRenderable>::BLOCK,
            ),
        };
        // Her row first is a stable position (the roster sorts the rest by name), so the bytes
        // stay the same turn to turn.
        Grounded::new(mine, others, delivery.truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::rag_budget::{RagDelivery, RagItem};

    struct Roster(Vec<RagItem>);

    #[async_trait::async_trait]
    impl RagSource for Roster {
        fn source_id(&self) -> &'static str {
            "roster"
        }
        fn expand_command(&self) -> Option<&'static str> {
            None // a stub: nothing further to fetch
        }
        async fn deliver(&self, _ctx: &RagContext) -> RagDelivery {
            RagDelivery::of("roster", self.0.clone(), false)
        }
    }

    /// The keys the faculty receives, through the one adapter it reads.
    async fn keys(here: WhoIsHere, me: uuid::Uuid) -> Vec<String> {
        let required = crate::cognition::rag_budget::Required(Arc::new(here));
        RagSource::deliver(&required, &RagContext::for_persona(me, 0))
            .await
            .items
            .into_iter()
            .map(|item| item.key)
            .collect()
    }

    // what this catches: "who is here" empty where no shared roster exists (her mind room, a
    // cold projection), which made every such turn ungrounded: she is present, so her row is.
    #[tokio::test]
    async fn with_no_shared_roster_she_is_still_here() {
        let me = uuid::Uuid::from_u128(1);
        let here = WhoIsHere::new(Arc::new(Roster(Vec::new())), me, "Atlas");
        assert_eq!(
            keys(here, me).await,
            vec![me.to_string()],
            "her row, and only hers"
        );
    }

    // what this catches: her row duplicated, or moved, when the shared roster already lists her:
    // she appears once, first, and the others keep the roster's order.
    #[tokio::test]
    async fn where_the_roster_lists_her_she_appears_once_first() {
        let me = uuid::Uuid::from_u128(1);
        let peer = uuid::Uuid::from_u128(2);
        let item = |id: uuid::Uuid| RagItem::whole(id.to_string(), "row", serde_json::json!({}));
        let here = WhoIsHere::new(Arc::new(Roster(vec![item(peer), item(me)])), me, "Atlas");
        assert_eq!(keys(here, me).await, vec![me.to_string(), peer.to_string()]);
    }
}
