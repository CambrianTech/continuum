use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `POST /train`'s body (fork `tools/server/server-train.cpp` `server_trainer::start`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrainRequest {
    pub examples: Vec<EngineExample>,
    /// a bare `<job>.gguf` name; the engine writes it inside the lane's `--train-dir`
    pub out: String,
    pub rank: u32,
    /// the engine reads a number > 0, so a fractional alpha is expressible
    pub alpha: f32,
    /// GGUF module names, comma-separated
    pub targets: String,
    pub window: u32,
    pub epochs: u32,
    pub lr: f64,
    pub val_split: f32,
    pub seed: u32,
    /// adapt only the last K blocks (fork #27); omitted = every block
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_layers: Option<u32>,
    /// what training may add on the GPU: the job's governed lease, which the engine enforces
    /// before allocating
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_budget_mib: Option<u64>,
    /// The bound on her decode slowdown while training runs, parts per million (fork #36). Sent
    /// instead of the time share (the engine refuses both).
    pub max_slowdown_ppm: u32,
    /// How an example longer than the window is fitted (fork #29).
    pub fit: Fit,
    /// The exact walk (fork #47): one optimizer step per window on the gradient carried back
    /// through every chunk's cached K/V and a hybrid's recurrent state.
    pub exact: bool,
    /// The exact walk's host memory per window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub walk_host_budget_mib: Option<u64>,
    /// The training chunk (one batch, one ubatch). An engine before the key takes 512.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunk: Option<u32>,
    /// Per-layer recompute (fork #37): a chunk holds one layer's activations, not every layer's.
    pub recompute: bool,
}

/// How an example longer than the window is fitted (fork #29).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// keep the system and tool head and her reply; drop the OLDEST history exchanges
    Middle,
    /// cut from the front
    Left,
}

/// One `/train` example: a prompt/completion pair, or a served conversation (OpenAI message shape
/// exactly as serving framed it, with the tools she was offered).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum EngineExample {
    Pair {
        prompt: String,
        completion: String,
    },
    Conversation {
        messages: Vec<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tools: Option<Value>,
    },
}

/// The body of `POST /train/pause` and `/train/resume`: which job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct JobRef {
    pub out: String,
}

/// `GET /train`'s run state. A state this core does not know is `Unknown`, never guessed into a
/// known one (it counts as possibly running).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrainState {
    Idle,
    /// claimed and preparing its examples, before it is accepted (fork #55)
    Preparing,
    Starting,
    Running,
    Done,
    Cancelled,
    Error,
    /// the server has no trainer (started without `--train-dir`)
    Unavailable,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EpochReport {
    pub train_loss: f64,
    /// null when no held-out token was evaluated (fork #31)
    #[serde(default)]
    pub eval_loss: Option<f64>,
}

/// `GET /train`'s body. `state` is required: a body without one does not parse, and an
/// unparsable status is retried like a lost one, never read as "not ours".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrainStatus {
    pub state: TrainState,
    #[serde(default)]
    pub out: Option<String>,
    #[serde(default)]
    pub batch: Option<u64>,
    #[serde(default)]
    pub batch_max: Option<u64>,
    #[serde(default)]
    pub epochs: Vec<EpochReport>,
    #[serde(default)]
    pub trainable_tokens: Option<u64>,
    /// the held-out trainable tokens the eval loss is measured over (fork #31)
    #[serde(default)]
    pub eval_trainable_tokens: Option<u64>,
    /// the training graph the engine measured before allocating it
    #[serde(default)]
    pub graph_mib: Option<f64>,
    /// what the training context itself held on the device (fork #52)
    #[serde(default)]
    pub context_mib: Option<f64>,
    /// the chunk the engine trained at: the shape its footprint is filed under
    #[serde(default)]
    pub chunk: Option<u32>,
    /// fork #28: a pause asked for, and one the worker has reached
    #[serde(default)]
    pub pause_requested: bool,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub waiting_for_serving: bool,
    /// the share the run took of the lane (fork #32)
    #[serde(default)]
    pub yielded_ms: Option<u64>,
    #[serde(default)]
    pub windows: Option<u64>,
    #[serde(default)]
    pub windows_while_busy: Option<u64>,
    #[serde(default)]
    pub share_ppm: Option<u64>,
    /// the slowdown bound and the measured rates (fork #36)
    #[serde(default)]
    pub max_slowdown_ppm: Option<u64>,
    #[serde(default)]
    pub decode_tps_no_window: Option<f64>,
    #[serde(default)]
    pub decode_tps_in_window: Option<f64>,
    #[serde(default)]
    pub slowdown_ppm_realized: Option<u64>,
    /// the model's block count and the blocks this run adapts (fork #27)
    #[serde(default)]
    pub n_layer: Option<u32>,
    #[serde(default)]
    pub layers_adapted: Option<u32>,
    /// examples kept, cut from the front to fit the window, and skipped (fork #29)
    #[serde(default)]
    pub examples: Option<u64>,
    #[serde(default)]
    pub examples_truncated: Option<u64>,
    #[serde(default)]
    pub examples_skipped: Option<u64>,
    /// which examples the run skipped, by their index in what the core sent (fork #51)
    #[serde(default)]
    pub skipped_indices: Vec<usize>,
    /// the window the engine's training graph actually used (fork #30)
    #[serde(default)]
    pub window: Option<u32>,
    #[serde(default)]
    pub error: Option<String>,
}

impl TrainStatus {
    /// What the run adds on the device: its graph and its context (fork #52). 0 when the engine
    /// reported no graph (nothing measured); an engine before #52 reports no context.
    pub fn run_bytes(&self) -> u64 {
        match self.graph_mib {
            Some(graph) => ((graph + self.context_mib.unwrap_or(0.0)) * 1024.0 * 1024.0) as u64, // unwrap_or: an engine before #52 reports no context
            None => 0,
        }
    }
}
