use super::*;

// ---------------------------------------------------------------------------
// benchmark/fetch — stage a catalogued suite so it can actually be run (#370)
// ---------------------------------------------------------------------------
/// Where a catalogued suite's rows actually live, and whether anything in this tree can read
/// them. Four states, because the four have four different fixes and collapsing any two of
/// them produces a refusal the operator has to go do archaeology on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceReach {
    /// Servable by the HuggingFace rows API at these exact coordinates.
    Rows {
        dataset: &'static str,
        config: &'static str,
        split: &'static str,
    },
    /// HuggingFace-hosted, but a LOADING-SCRIPT dataset: the rows API refuses it outright
    /// ("runs arbitrary Python code"). No config or split makes it work, so a refusal that
    /// merely says "not found" invites an infinite guessing loop. Measured live 2026-08-19
    /// against `datasets-server.huggingface.co/splits` for both rows carrying this.
    HuggingFaceScriptDataset { dataset: &'static str },
    /// A real source, just not one the HF path can read (GitHub raw files, a repo to clone).
    /// Needs its own fetcher; naming that is the honest answer.
    ForeignSource { url: &'static str },
    /// Ships with the binary — there is nothing to pull.
    InTree,
}

impl BenchmarkSpec {
    /// The suite's fetch coordinates, as DATA rather than as an operator's memory.
    ///
    /// `config`/`split` are NOT derivable from the URL and are not uniformly `default`/`test`
    /// — bigcodebench versions its splits (`v0.1.4`), and a wrong guess returns an in-band HF
    /// error that a caller reads as "the suite is empty". So the exceptions live here, in the
    /// ONE place that knows, exactly as [`BenchmarkSpec::swe_dataset`] already does for row
    /// shape. The dataset id is still read back off `source_url` so it is never duplicated.
    pub fn reach(&self) -> SourceReach {
        let Some(url) = self.source_url else {
            return SourceReach::InTree;
        };
        let Some(dataset) = url
            .strip_prefix("https://huggingface.co/datasets/")
            .filter(|id| id.contains('/'))
        else {
            return SourceReach::ForeignSource { url };
        };
        // Loading-script datasets: the rows API cannot serve these at ANY coordinates.
        if matches!(dataset, "codeparrot/apps" | "livecodebench/code_generation_lite") {
            return SourceReach::HuggingFaceScriptDataset { dataset };
        }
        let (config, split) = match dataset {
            // bigcodebench publishes revisions as SPLITS; `test` does not exist. v0.1.4 is
            // the newest as of 2026-08-19 — bump it here when they publish, so the version
            // scored against is a recorded catalog fact and not whatever the default was.
            "bigcode/bigcodebench" => ("default", "v0.1.4"),
            _ => ("default", "test"),
        };
        SourceReach::Rows {
            dataset,
            config,
            split,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkFetchParams.ts"
)]
pub struct BenchmarkFetchParams {
    /// Which catalogued benchmark to stage, as it appears in `benchmark/list`.
    pub benchmark: String,
    /// Dataset config. Defaults to `default` — the HF convention.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub config: Option<String>,
    /// Split to pull. Defaults to `test`, which is what a benchmark is scored on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub split: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkFetchResult.ts"
)]
pub struct BenchmarkFetchResult {
    pub benchmark: String,
    pub dataset: String,
    pub config: String,
    pub split: String,
    /// Rows actually staged. This is the suite's REAL denominator — compare it against the
    /// catalog's `tasks` before trusting any rate computed from it.
    pub rows: usize,
    /// The catalog's declared task count, so a mismatch is visible at fetch time rather than
    /// discovered when a published number turns out to be over the wrong denominator.
    pub declared_tasks: u32,
    /// True when `rows` and `declared_tasks` agree. False is not fatal — datasets are revised
    /// upstream — but a rate published over a disagreeing denominator is not comparable.
    pub denominator_matches: bool,
    /// How many rows actually PROJECT into posable tasks through this suite's `SuiteAdapter`.
    /// `None` = the suite has no adapter yet: its rows are staged but cannot be posed to a
    /// citizen. Staged-but-unposable is the honest middle state, and reporting it as a distinct
    /// value is what keeps a fetched suite from LOOKING runnable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tasks: Option<usize>,
    /// Present only when projection is unavailable or failed, saying which of those it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub adapter_note: Option<String>,
}

#[derive(Default)]
pub struct BenchmarkFetch;
