//! `PersonaModelOverride` — the persisted, runtime-mutable per-persona base-model
//! assignment.
//!
//! ### The concern this owns
//!
//! A persona's *default* base model is immutable catalog data: the allocator reads
//! `PersonaCatalogEntry.model_preferences` (tiered by VRAM) and picks the best that
//! fits the host. That is the right default, but it is not *re-assignable at
//! runtime* — there is no way for an operator (or the persona herself, as a tool)
//! to say "from now on, run Asha on the 14B coder" and have it stick across
//! restarts. This module is that missing binding: a single durable record per
//! persona, **read by the allocator at the highest precedence**, written by
//! `persona/reassign-model`.
//!
//! ### Where it lives, and why
//!
//! The record is one file at [`PersonaHome::model_override_json`] —
//! `<home>/model_override.json`. Living under the persona's home root means it
//! rides the [`PersonaHomeBundle`](crate::persona::portability::PersonaHomeBundle)
//! automatically: move the home to another grid node and the assignment travels
//! with the self (per [[persona-persistence-self-determination]]). It is the
//! per-persona dual of the host-level force-serve pin (`serving/pin`): the pin says
//! "this *host* serves model Y"; the override says "this *persona* is assigned model
//! Y". `persona/reassign-model` composes both — it writes the override here AND pins
//! the host so the assignment is real now.
//!
//! ### Not to be confused with the per-request `model_override`
//!
//! `cognition::shared_analysis::types::model_override` is a *transient* per-call
//! routing hint ("use model X for THIS analysis call"). This is the *persisted*
//! per-persona preference. Different lifetimes, different layers — kept in separate
//! types on purpose.
//!
//! ### Fail loud, never swallow
//!
//! Per [[fallbacks-are-illegal-fail-loud]]: a missing file is `Ok(None)` (the
//! persona simply has no override), but a *malformed* or *unreadable* file is a hard
//! [`PersonaModelOverrideError`] naming the path — never silently treated as "no
//! override", which would hide a corrupted assignment behind the catalog default.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::persona::home::PersonaHome;

/// The persisted per-persona base-model assignment: which model, set when, by whom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonaModelOverride {
    /// The base model id (as it appears in `models/list`) this persona is assigned
    /// to. Takes precedence over the catalog's `model_preferences`.
    pub model_id: String,
    /// Unix-ms when the assignment was made — for audit / "since when" display.
    pub set_at_ms: u64,
    /// Who made the assignment: an operator user-id, or the persona's own id when
    /// she reassigned herself as a tool. `None` when the origin is unknown.
    pub set_by: Option<String>,
}

/// Failure modes of reading / writing a persona's model override. Every variant
/// names the path so the operator can act on it — never a silent default.
#[derive(Debug, thiserror::Error)]
pub enum PersonaModelOverrideError {
    /// The override file could not be read or written (permissions, disk, etc.).
    #[error("persona model override I/O at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The override file exists but is not valid JSON for [`PersonaModelOverride`].
    /// Surfaced loud rather than swallowed — a corrupt assignment must not silently
    /// fall through to the catalog default.
    #[error("persona model override at {path} is malformed: {source}")]
    Malformed {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

impl PersonaModelOverride {
    /// Construct a fresh assignment stamped at `now_ms`. `set_by` is the operator
    /// user-id or the persona's own id (when self-assigning).
    pub fn new(model_id: impl Into<String>, set_by: Option<String>, now_ms: u64) -> Self {
        Self {
            model_id: model_id.into(),
            set_at_ms: now_ms,
            set_by,
        }
    }

    /// Read the override for a persona home. `Ok(None)` if no override file exists
    /// (the common case — the persona uses her catalog default); `Err` on an
    /// unreadable or malformed file. A corrupt assignment fails loud, never silently
    /// degrades to the default.
    pub fn load(home: &PersonaHome) -> Result<Option<Self>, PersonaModelOverrideError> {
        let path = home.model_override_json();
        match std::fs::read(&path) {
            Ok(bytes) => {
                let parsed = serde_json::from_slice(&bytes).map_err(|source| {
                    PersonaModelOverrideError::Malformed {
                        path: path.clone(),
                        source,
                    }
                })?;
                Ok(Some(parsed))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(PersonaModelOverrideError::Io { path, source }),
        }
    }

    /// Atomically persist this assignment for a persona home (write to a sibling
    /// `.tmp`, fsync, then rename over the target). The home directory is created if
    /// absent. A crash mid-write leaves the previous assignment (if any) intact and
    /// the tmp file on disk for inspection — never a half-written override.
    pub fn write(&self, home: &PersonaHome) -> Result<(), PersonaModelOverrideError> {
        use std::io::Write as _;

        let path = home.model_override_json();
        home.ensure_exists()
            .map_err(|source| PersonaModelOverrideError::Io {
                path: path.clone(),
                source,
            })?;

        let json = serde_json::to_vec_pretty(self).map_err(|source| {
            PersonaModelOverrideError::Malformed {
                path: path.clone(),
                source,
            }
        })?;

        let tmp_path = path.with_extension("json.tmp");
        let mut file =
            std::fs::File::create(&tmp_path).map_err(|source| PersonaModelOverrideError::Io {
                path: tmp_path.clone(),
                source,
            })?;
        file.write_all(&json)
            .map_err(|source| PersonaModelOverrideError::Io {
                path: tmp_path.clone(),
                source,
            })?;
        file.sync_all()
            .map_err(|source| PersonaModelOverrideError::Io {
                path: tmp_path.clone(),
                source,
            })?;
        std::fs::rename(&tmp_path, &path).map_err(|source| PersonaModelOverrideError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(())
    }

    /// Clear any assignment for a persona home, returning her to the catalog default.
    /// Idempotent: clearing when no override is set is `Ok(())`, not an error.
    pub fn clear(home: &PersonaHome) -> Result<(), PersonaModelOverrideError> {
        let path = home.model_override_json();
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(PersonaModelOverrideError::Io { path, source }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> (tempfile::TempDir, PersonaHome) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let home = PersonaHome::for_persona(tmp.path(), "Asha");
        (tmp, home)
    }

    // what this catches: an absent override reads as Ok(None) — a persona with no
    // assignment is the common case and must NOT be an error (that would break every
    // allocation for a persona who never reassigned).
    #[test]
    fn absent_override_is_none_not_error() {
        let (_tmp, home) = home();
        let loaded = PersonaModelOverride::load(&home).expect("absent override is not an error");
        assert!(loaded.is_none());
    }

    // what this catches: write→load round-trips the full record (model + timestamp +
    // origin) — the durable assignment the allocator reads back is exactly what was
    // written.
    #[test]
    fn write_then_load_round_trips() {
        let (_tmp, home) = home();
        let ov = PersonaModelOverride::new(
            "qwen3-coder-14b",
            Some("operator".into()),
            1_700_000_000_123,
        );
        ov.write(&home).expect("write succeeds");

        let loaded = PersonaModelOverride::load(&home)
            .expect("load succeeds")
            .expect("an override is present after write");
        assert_eq!(loaded, ov);
        assert_eq!(loaded.model_id, "qwen3-coder-14b");
        assert_eq!(loaded.set_by.as_deref(), Some("operator"));
        assert_eq!(loaded.set_at_ms, 1_700_000_000_123);
    }

    // what this catches: writing twice replaces (not appends/duplicates) — a
    // reassignment overwrites the prior assignment so there is exactly ONE current
    // model per persona.
    #[test]
    fn second_write_replaces_the_first() {
        let (_tmp, home) = home();
        PersonaModelOverride::new("model-a", None, 1)
            .write(&home)
            .expect("first write");
        PersonaModelOverride::new("model-b", None, 2)
            .write(&home)
            .expect("second write");

        let loaded = PersonaModelOverride::load(&home)
            .expect("load")
            .expect("present");
        assert_eq!(loaded.model_id, "model-b", "the latest assignment wins");
        assert_eq!(loaded.set_at_ms, 2);
    }

    // what this catches: clear removes the assignment (back to catalog default) and
    // is idempotent — clearing twice, or clearing when none is set, is never an error.
    #[test]
    fn clear_removes_and_is_idempotent() {
        let (_tmp, home) = home();
        PersonaModelOverride::new("model-x", None, 1)
            .write(&home)
            .expect("write");
        assert!(PersonaModelOverride::load(&home).expect("load").is_some());

        PersonaModelOverride::clear(&home).expect("first clear");
        assert!(
            PersonaModelOverride::load(&home)
                .expect("load after clear")
                .is_none(),
            "cleared override is gone → catalog default"
        );
        PersonaModelOverride::clear(&home).expect("clearing an absent override is idempotent");
    }

    // what this catches: a malformed override file fails LOUD (Malformed naming the
    // path), never silently treated as "no override". A corrupt assignment hiding
    // behind the catalog default is exactly the fallback this doctrine forbids.
    #[test]
    fn malformed_override_fails_loud() {
        let (_tmp, home) = home();
        home.ensure_exists().expect("mkdir");
        std::fs::write(home.model_override_json(), b"{ this is not json").expect("seed garbage");

        let err = PersonaModelOverride::load(&home).expect_err("malformed must fail loud");
        assert!(
            matches!(err, PersonaModelOverrideError::Malformed { .. }),
            "got {err:?}"
        );
    }
}
