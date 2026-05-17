//! VDD harness support.
//!
//! Harnesses emit machine-readable records plus replay artifacts. A missing
//! live prerequisite is a typed result, not a passing fallback.

pub mod artifacts;
pub mod chat_roundtrip;
pub mod record;

pub use artifacts::{ArtifactBundle, ArtifactWriter};
pub use chat_roundtrip::{
    ChatRoundtripConfig, ChatRoundtripHarness, ChatRoundtripObservation, ChatRoundtripProbe,
    LiveChatProbe,
};
pub use record::{HarnessStatus, StandardVddRecord, VddError};
