//! VDD harness support.
//!
//! Harnesses emit machine-readable records plus replay artifacts. A missing
//! live prerequisite is a typed result, not a passing fallback.

pub mod artifacts;
pub mod chat_roundtrip;
pub mod reader;
pub mod record;
pub mod registry;

pub use artifacts::{ArtifactBundle, ArtifactWriter};
pub use chat_roundtrip::{
    ChatRoundtripConfig, ChatRoundtripHarness, ChatRoundtripObservation, ChatRoundtripProbe,
    LiveChatProbe,
};
pub use reader::{latest_per_scenario, read_records, VddReadOptions, VddRecordEntry};
pub use record::{HarnessStatus, StandardVddRecord, VddError};
pub use registry::{HARNESS_SPECS, HarnessCadence, HarnessId, HarnessSpec, harness_spec};
