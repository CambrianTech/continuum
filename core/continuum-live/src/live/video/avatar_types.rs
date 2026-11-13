//! Pure avatar types — compiled in EVERY build, with or without `avatar-3d`.
//!
//! The expression/gesture vocabulary and the render-slot/resolution constants are
//! read by code that has nothing to do with drawing a 3D model: sentiment and
//! emotion-tag parsing, cognitive animation cues, the `avatar/snapshot` params,
//! the video-track dimensions. They used to live inside `bevy_renderer`, which
//! made every one of those callers depend on Bevy. They live here now so a
//! headless / CPU-only build (no `avatar-3d` feature) keeps the vocabulary while
//! the renderer itself is compiled out. `bevy_renderer` re-exports them, so its
//! old paths keep working when the feature is on.

/// Maximum number of concurrent avatar render slots.
pub const MAX_AVATAR_SLOTS: u8 = 16;

/// Default render resolution per avatar.
pub const AVATAR_WIDTH: u32 = 640;
pub const AVATAR_HEIGHT: u32 = 360;

/// Emotional expression state for avatar facial animation.
/// Maps to VRM expression blend shape presets. Neutral = no expression active.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
    ts_rs::TS,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/avatar/Emotion.ts")]
pub enum Emotion {
    #[default]
    Neutral,
    Happy,
    Sad,
    Angry,
    Surprised,
    Relaxed,
}

/// Body gesture for avatar upper-body animation.
/// Driven by speech content analysis — gestures fire alongside emotions
/// since they animate different body parts (arms vs face).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
    ts_rs::TS,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/avatar/Gesture.ts")]
pub enum Gesture {
    #[default]
    None,
    /// Friendly wave — right arm up, forearm oscillates
    Wave,
    /// Thinking pose — right hand near chin, head tilts
    Think,
    /// Emphatic head nod — stronger than speech nod
    Nod,
    /// Shoulders up, arms slightly out — uncertainty
    Shrug,
    /// Right arm extended forward — directing attention
    Point,
    /// Both arms slightly out, palms up — explaining
    OpenHands,
}
