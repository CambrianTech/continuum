//! `hf/search-datasets` — find training datasets on the HuggingFace Hub.

use super::{search_hub, HfSearchParams, HfSearchResult, HubKind};

crate::action_command! {
    /// Search HuggingFace Hub for DATASETS by keyword — instruction corpora,
    /// dialogue sets, domain data to learn from. Returns the top dataset repos
    /// with downloads, likes and tags, each with its Hub URL and the
    /// `<owner>/<name>` id you can import for training. Use this to forage for
    /// data to close a knowledge gap or feed an academy course. Example query:
    /// "function calling conversational". Reliable public search — no key needed.
    pub struct HfSearchDatasets;
    name: "hf/search-datasets",
    access: AiSafe,
    params: HfSearchParams,
    output: HfSearchResult,
    run(_this, _ctx, p) => { search_hub(HubKind::Datasets, p).await }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the wire name + non-empty dataset-facing description — the
    // persona is offered "hf/search-datasets" as a tool whose guidance points at
    // datasets/training (not models), and the name is the routing key. A name drift
    // here unwires the foraging hand silently.
    #[test]
    fn name_and_description() {
        assert_eq!(HfSearchDatasets::NAME, "hf/search-datasets");
        assert!(HfSearchDatasets::DESCRIPTION.contains("DATASETS"));
        assert!(HfSearchDatasets::DESCRIPTION.contains("training"));
    }
}
