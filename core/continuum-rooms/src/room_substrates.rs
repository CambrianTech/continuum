//! The process's per-room positron substrates: the chat projection writes each room's view
//! into them and a persona's grounding reads them. The readers are the mind, so the one
//! registry lives here and the socket layer that writes it looks it up.

/// THE per-room substrate registry for this process (#408).
///
/// A process-global `OnceLock`, the same shape as
/// `ipc::positron_nav_source::global_nav_focus` and the channel-bookmarks singleton —
/// because the WRITER (the chat projection, in the WS boot block) and the READERS
/// (a persona's grounding, bound at spawn in `persona::supervisor`) are constructed
/// in different places and must land on ONE registry. Two registries would be two
/// stores, which is the exact defect this repair exists to remove.
///
/// It is a registry of `Arc`-shared substrates, so this is a handle lookup, not a
/// cache: cloning it clones `Arc`s.
pub fn global_room_substrates() -> std::sync::Arc<continuum_positron::scoping::PerRoomSubstrates> {
    use std::sync::OnceLock;
    static G: OnceLock<std::sync::Arc<continuum_positron::scoping::PerRoomSubstrates>> =
        OnceLock::new();
    G.get_or_init(|| std::sync::Arc::new(continuum_positron::scoping::PerRoomSubstrates::new()))
        .clone()
}
