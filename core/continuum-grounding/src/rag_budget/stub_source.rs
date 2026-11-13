use super::*;

/// Stub source for tests. Holds a Vec of pre-built RagItems and delivers them all, re-deriving
/// from the start on every call (ruling a: no positional cursor; truncation is notification
/// metadata, not a resumable handle). Also demonstrates persona-scoped delivery: calls with a
/// mismatched ctx.persona_id return empty.
pub struct StubRagSource {
    pub(super) source_id: &'static str,
    pub(super) persona_id: uuid::Uuid,
    pub(super) items: Vec<RagItem>,
}

impl StubRagSource {
    pub fn new(source_id: &'static str, persona_id: uuid::Uuid, items: Vec<RagItem>) -> Self {
        Self { source_id, persona_id, items }
    }
}

#[async_trait]
impl RagSource for StubRagSource {
    fn source_id(&self) -> &'static str {
        self.source_id
    }

    fn expand_command(&self) -> Option<&'static str> {
        // Test/stub source: nothing further to fetch.
        None
    }

    async fn deliver(&self, ctx: &RagContext) -> RagDelivery {
        // Defense-in-depth identity check: another persona's ctx gets nothing.
        if ctx.persona_id != self.persona_id {
            return RagDelivery::empty(self.source_id);
        }
        RagDelivery::of(self.source_id, self.items.clone(), false)
    }
}
