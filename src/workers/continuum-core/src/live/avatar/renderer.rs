//! AvatarRenderer trait — the universal interface for avatar frame production.
//!
//! Every rendering backend (procedural, Bevy 3D, Live2D, etc.) implements this trait.
//! The render loop and LiveKit video pipeline consume AvatarRenderer without knowing
//! which backend produced the frame.

use super::frame::RgbaFrame;

/// Trait for avatar rendering backends.
/// Each implementation produces RGBA frames at the configured FPS.
pub trait AvatarRenderer: Send + Sync {
    /// Render a single RGBA frame. Called at the configured FPS.
    fn render_frame(&mut self) -> RgbaFrame;

    /// Notify the renderer that the persona is speaking (for lip sync, expressions).
    fn set_speaking(&mut self, _speaking: bool) {}

    /// Set the current viseme for lip sync (0-14, OCULUS viseme set).
    fn set_viseme(&mut self, _viseme: u8, _weight: f32) {}

    /// Set the current emotion expression.
    fn set_emotion(&mut self, _emotion: &str) {}
}
