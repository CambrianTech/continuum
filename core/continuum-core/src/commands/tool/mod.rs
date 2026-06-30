//! `tool/*` — the persona's affordances over her OWN tool usage. Today that is
//! `tool/output`: recover and grep a flood-sized tool result the executor spilled
//! to disk (tier 2 of the flood protection). Stateless and AiSafe; each verb
//! scopes to the calling persona's own id, so this is a personal, not shared,
//! surface.

pub mod output;
