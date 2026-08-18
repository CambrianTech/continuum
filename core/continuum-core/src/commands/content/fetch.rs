//! `content/fetch` — read part of content held behind a handle.
//!
//! The dereference half of [`crate::content`]. When something is too large to hand over
//! whole, its producer parks it and returns a header plus a handle; this is how a citizen
//! then reads it, at whatever pace her window allows.
//!
//! ## Why this is NOT on the native surface
//!
//! It looked like a core act, so it shipped `NATIVE` — and the agentic-surface ratchet
//! immediately caught it: 8,040 → 11,608 tokens against an 11,300 ceiling. Paying for a
//! full schema in EVERY prompt is the #333 defect, and this verb is the wrong place to
//! spend it, because it is only meaningful on the turns where she actually holds a handle.
//!
//! It does not need to be resident, because [`ContentHeader::fetch_with`] names the exact
//! call AT THE MOMENT a handle is issued — the producer tells her the call form precisely
//! when it becomes relevant. She is aware of the verb regardless: the compact catalog
//! lists every authorized command by name, and `commands/help` expands this one on demand.
//!
//! Which is the same principle the module itself is built on, applied one level up: do not
//! hold the detail resident, carry the pointer and drill in when something warrants it.
//!
//! [`ContentHeader::fetch_with`]: crate::content::ContentHeader::fetch_with
//!
//! This command decides NOTHING about the content. It looks the source up and calls its
//! method; whether a span means lines, entries, or "the whole thing or nothing" is the
//! source's answer, and a refusal here is written by the producer that knows why.

use uuid::Uuid;

use std::sync::Arc;

use crate::content::{ContentRegistry, Span};
use crate::sdk_codegen::CommandError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/content/ContentFetchParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ContentFetchParams {
    /// The handle's id, from the header you were given.
    #[ts(type = "string")]
    pub handle: Uuid,
    /// First unit to read, 1-based, in the units the header named (line, entry).
    /// Defaults to the beginning.
    #[serde(default = "default_from")]
    pub from: usize,
    /// How many units. The source clamps to what it has and tells you what it covered.
    #[serde(default = "default_count")]
    pub count: usize,
}

/// Start at the beginning — the overwhelmingly common first call, and a 0 here would be
/// out of range in 1-based units.
fn default_from() -> usize {
    1
}

/// A page, when the caller does not say. Not a context bound — the caller's window decides
/// how much she asks for, and this is only the value used when she asks for none.
fn default_count() -> usize {
    100
}

#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/content/ContentFetchResult.ts"
)]
pub struct ContentFetchResult {
    /// The content, whole in its own units.
    pub body: String,
    /// First unit this covers, after clamping.
    pub from: usize,
    /// How many units this covers, after clamping.
    pub count: usize,
    /// Where to continue, or `null` when you have reached the end. `null` is the signal
    /// that you have seen ALL of it — the thing a truncated copy can never tell you.
    #[ts(optional)]
    pub next_from: Option<usize>,
}

crate::action_command! {
    /// Read part of content held behind a handle. Use the handle and units from the
    /// header you were given (`from`/`count` in lines or entries). `nextFrom` tells you
    /// where to continue; when it is absent you have read everything.
    pub struct ContentFetch { registry: Arc<ContentRegistry> }
    name: "content/fetch",
    access: AiSafe,
    params: ContentFetchParams,
    output: ContentFetchResult,
    run(this, _ctx, p) => {
        if p.count == 0 {
            return Err(CommandError::Invalid(
                "count must be at least 1 — ask for the units you want to read".to_string(),
            ));
        }
        let slice = this.registry.fetch(p.handle, Span { from: p.from, count: p.count })
            // The source's own words: a released handle, an out-of-range ask, or an
            // indivisible payload refusing a partial read. Never re-worded here — the
            // producer is the party that knows why, and its message names the remedy.
            .map_err(CommandError::Invalid)?;
        Ok(ContentFetchResult {
            body: slice.body,
            from: slice.covered.from,
            count: slice.covered.count,
            next_from: slice.next.map(|n| n.from),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::ListingContent;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: this verb creeping onto the NATIVE surface. It shipped native
    // once and blew the agentic-surface ceiling by 308 tokens (8,040 → 11,608 / 11,300) —
    // a full schema in every prompt for a verb that only matters on turns where she holds
    // a handle. The header's `fetch_with` names the call when it becomes relevant, and the
    // catalog keeps her aware of it meanwhile; AiSafe is what actually makes it callable.
    #[test]
    fn it_is_ai_safe_but_not_resident_because_the_header_names_the_call_when_it_matters() {
        assert_eq!(ContentFetch::NAME, "content/fetch");
        assert_eq!(ContentFetch::ACCESS, AccessLevel::AiSafe);
        assert!(
            !ContentFetch::NATIVE,
            "must stay OFF the native surface — every prompt would pay for a schema that \
             is relevant only when a handle is in hand (#333)"
        );
    }

    // what this catches: the end-of-content signal getting lost in the wire type. `next`
    // is how a reader learns she has seen ALL of it; if it never surfaces as `nextFrom`
    // she cannot distinguish "that's everything" from "that's the part you were given",
    // which is exactly the confusion this whole design exists to end.
    #[tokio::test]
    async fn the_result_carries_where_to_continue_and_where_to_stop() {
        let registry = Arc::new(ContentRegistry::default());
        let (handle, _) = registry.publish(Arc::new(ListingContent::new(
            "listing",
            "3 files",
            vec!["a".into(), "b".into(), "c".into()],
        )));
        let id: Uuid = handle.id.into();
        let cmd = ContentFetch { registry };
        let ctx = crate::sdk_codegen::Ctx::default();

        let page = cmd
            .run(&ctx, ContentFetchParams { handle: id, from: 1, count: 2 })
            .await
            .expect("first page");
        assert_eq!(page.body, "a\nb");
        assert_eq!(page.next_from, Some(3), "says exactly where to continue");

        let last = cmd
            .run(&ctx, ContentFetchParams { handle: id, from: 3, count: 2 })
            .await
            .expect("last page");
        assert_eq!(last.count, 1, "clamped to what exists");
        assert!(last.next_from.is_none(), "and reports the end as the end");
    }
}
