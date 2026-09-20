//! `data/list` — read records from a collection (persona/UI-facing).

use std::sync::Arc;

use crate::modules::data::{DataListParams, DataListResult, DataState};

crate::action_command! {
    /// List records from a data collection — rooms, users, messages, and any
    /// other entity store. Name the `collection` and optionally an intuitive
    /// plain-JSON `filter` (e.g. `{"roomId": "general"}`), ordering, and paging.
    /// Returns the matching records plus an accurate `total` (an exact count of
    /// everything matching the filter, independent of `limit`). Reading shared
    /// state is a read — gated `AiSafe`.
    pub struct DataList { state: Arc<DataState> }
    name: "data/list",
    access: AiSafe,
    params: DataListParams,
    output: DataListResult,
    run(this, _ctx, p) => {
        // The storage logic lives on DataState (the module owns its compute);
        // this command is the thin typed wrapper. A bad filter / failed query
        // surfaces as a loud error from `list`, never a silent empty result.
        let result = this.state.list(p).await?;
        Ok(result)
    }
}
