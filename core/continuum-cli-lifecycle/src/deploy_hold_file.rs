//! The one owner of the node's deploy hold file, `state/deploy-hold` (a [`Hold`]): the tracker reads it
//! through [`read`], and every change goes through [`change`], one locked read-decide-apply, so a
//! candidate's hold never overwrites or releases an operator's, and two writers never interleave (Codex
//! on 17660bf67f: a JSON-only second reader took a legacy text hold for none and could overwrite it).

use std::io;
use std::path::{Path, PathBuf};

use crate::deploy_tracker::{Hold, HoldOwner};

/// What the hold file says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldRead {
    /// No file: nothing holds the tracker.
    Missing,
    /// A hold: JSON, or a legacy bash-written text file read as a TTL-less hold whose reason is its text.
    Held(Hold),
    /// The file exists and cannot be read: it holds (never read as no hold), flagged with why.
    Unreadable(Unreadable),
}

/// Why the hold file could not be read as a typed hold: the facts, never formatted text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unreadable {
    /// The file is there but is not a typed hold (an older form `migrate` rewrites, or a stranger's).
    NotTyped { path: PathBuf, line: usize, column: usize },
    /// The file could not be read at all.
    Io { path: PathBuf, kind: io::ErrorKind },
}

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unreadable::NotTyped { path, line, column } => write!(
                f,
                "{}: not a typed hold (at line {line}, column {column}); `migrate` rewrites an older form",
                path.display()
            ),
            Unreadable::Io { path, kind } => write!(f, "{}: {kind}", path.display()),
        }
    }
}

impl HoldRead {
    /// The hold the tracker honors: an unreadable file holds, TTL-less, so it is honored and flagged stale.
    pub fn honored(self) -> Option<Hold> {
        match self {
            HoldRead::Missing => None,
            HoldRead::Held(hold) => Some(hold),
            HoldRead::Unreadable(why) => Some(Hold { reason: format!("the deploy hold file is unreadable: {why}"), created_ms: 0, ttl_ms: None, owner: HoldOwner::Operator }),
        }
    }
}

/// What a change does to the hold file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Keep,
    Write(Hold),
    Remove,
}

fn file(state_dir: &Path) -> PathBuf {
    state_dir.join("deploy-hold")
}

/// The hold file as it is now: the typed form only. An older form is Unreadable (it holds, flagged)
/// until [`migrate`] rewrites it; no default reads an old file as a typed one (Cormac on #6286).
pub fn read(state_dir: &Path) -> HoldRead {
    let path = file(state_dir);
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Hold>(&text) {
            Ok(hold) => HoldRead::Held(hold),
            Err(why) => HoldRead::Unreadable(Unreadable::NotTyped { path, line: why.line(), column: why.column() }),
        },
        Err(missing) if missing.kind() == io::ErrorKind::NotFound => HoldRead::Missing,
        Err(other) => HoldRead::Unreadable(Unreadable::Io { path, kind: other.kind() }),
    }
}

/// The hold as written before holds had an owner (bash's text file, or JSON without `owner`).
#[derive(serde::Deserialize)]
struct PreOwner {
    reason: String,
    created_ms: u64,
    #[serde(default)]
    ttl_ms: Option<u64>,
}

/// What [`migrate`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Migrated {
    /// Nothing to migrate: no file, or already typed.
    Nothing,
    /// An older hold rewritten in the typed form as an operator's, its reason, time and TTL kept.
    Rewritten(Hold),
    /// A file neither typed nor an older form: left as it is, so it holds and is flagged.
    Refused(Unreadable),
    /// The file was unreadable, and reading it again to migrate failed too.
    RefusedRereading { first: Unreadable, again: io::ErrorKind },
}

/// The one-shot migration of an older hold to the typed form, under the file's lock: JSON without an
/// owner keeps its fields; bash's text file becomes a TTL-less hold created when the file was written.
/// Run once when the tracker starts; idempotent.
pub fn migrate(state_dir: &Path) -> io::Result<Migrated> {
    let path = file(state_dir);
    let mut migrated = Migrated::Nothing;
    change(state_dir, |current| match current {
        HoldRead::Missing | HoldRead::Held(_) => Change::Keep,
        HoldRead::Unreadable(why) => match older(&path) {
            Ok(Some(hold)) => {
                migrated = Migrated::Rewritten(hold.clone());
                Change::Write(hold)
            }
            Ok(None) => {
                migrated = Migrated::Refused(why.clone());
                Change::Keep
            }
            Err(unreadable) => {
                migrated = Migrated::RefusedRereading { first: why.clone(), again: unreadable.kind() };
                Change::Keep
            }
        },
    })?;
    Ok(migrated)
}

/// The file as an older hold, rewritten typed; `None` when it is JSON of neither form (a stranger's).
fn older(path: &Path) -> io::Result<Option<Hold>> {
    let text = std::fs::read_to_string(path)?;
    if let Ok(old) = serde_json::from_str::<PreOwner>(&text) {
        return Ok(Some(Hold { reason: old.reason, created_ms: old.created_ms, ttl_ms: old.ttl_ms, owner: HoldOwner::Operator }));
    }
    if text.trim_start().starts_with('{') {
        return Ok(None);
    }
    let at = std::fs::metadata(path)?.modified()?.duration_since(std::time::UNIX_EPOCH).map_err(io::Error::other)?;
    Ok(Some(Hold { reason: text.trim().to_string(), created_ms: at.as_millis() as u64, ttl_ms: None, owner: HoldOwner::Operator }))
}

/// Read, decide with `decide`, and apply, under the hold file's own lock (`state/deploy-hold.lock`, taken
/// by waiting): the read and the write are one step for every writer. Returns what was applied.
pub fn change(state_dir: &Path, decide: impl FnOnce(&HoldRead) -> Change) -> io::Result<Change> {
    std::fs::create_dir_all(state_dir)?;
    let _one_writer = continuum_host::durable::lock_path(&state_dir.join("deploy-hold.lock"), true)?;
    let decided = decide(&read(state_dir));
    match &decided {
        Change::Keep => {}
        Change::Write(hold) => {
            let text = serde_json::to_vec(hold).map_err(io::Error::other)?;
            continuum_host::durable::write_replace(&file(state_dir), &text)?;
        }
        Change::Remove => match std::fs::remove_file(file(state_dir)) {
            Ok(()) => {}
            Err(gone) if gone.kind() == io::ErrorKind::NotFound => {}
            Err(other) => return Err(other),
        },
    }
    Ok(decided)
}


/// A measurer's lease on the node for one run (Codex on #6288): acquired before its handoff, renewed
/// through resident readiness and the window's real end, released when the run ends. The TTL bounds only
/// a measurer that is lost. The tracker infers nothing: a build is held because someone said so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acquired {
    /// The lease, and the token its renewals and release must carry.
    Leased { token: uuid::Uuid },
    /// An active hold is in the way (an operator's, another lease, or an unreadable file): not taken.
    Busy(HoldRead),
}

/// What a renewal or a release did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByToken {
    Done,
    /// The hold is not this acquisition's (released, expired and replaced, or never this token's).
    NotYours,
    /// This acquisition's lease had already expired: the tracker may have moved on, so it is not
    /// renewed (Codex on #6288); acquire again.
    Expired,
}

/// What a measurer asks for: the node, while `who` measures `build`, for `ttl_ms`.
#[derive(Debug, Clone, Copy)]
pub struct LeaseAsk<'a> {
    pub who: &'a str,
    pub build: &'a str,
    pub ttl_ms: u64,
}

/// The measurer asks for the node at `now`.
pub fn acquire(state_dir: &Path, ask: &LeaseAsk<'_>, clock: &dyn Fn() -> u64) -> io::Result<Acquired> {
    let LeaseAsk { who, build, ttl_ms } = *ask;
    let mut outcome = None;
    change(state_dir, |current| {
        // the time is read under the hold's lock, so no other writer's change can fall between them
        let now = clock();
        let free = match current {
            HoldRead::Missing => true,
            HoldRead::Held(hold) => !hold.is_active(now),
            HoldRead::Unreadable(_) => false,
        };
        if !free {
            outcome = Some(Acquired::Busy(current.clone()));
            return Change::Keep;
        }
        let token = uuid::Uuid::new_v4();
        outcome = Some(Acquired::Leased { token });
        Change::Write(Hold {
            reason: format!("{who} measures {build}"),
            created_ms: now,
            ttl_ms: Some(ttl_ms),
            owner: HoldOwner::Measurer { who: who.to_string(), build: build.to_string(), token },
        })
    })?;
    outcome.ok_or_else(|| io::Error::other("the hold change ran without deciding"))
}

/// Renew the lease `token` names for `ttl_ms` from `now`; any other hold is left as it is.
pub fn renew(state_dir: &Path, token: uuid::Uuid, ttl_ms: u64, clock: &dyn Fn() -> u64) -> io::Result<ByToken> {
    let mut outcome = ByToken::NotYours;
    change(state_dir, |current| match current {
        HoldRead::Held(hold) if leased_by(hold, token) => {
            // the time is read under the hold's lock: an expired lease is never revived, since the
            // tracker may already have moved on (Codex on #6288)
            let now = clock();
            if !hold.is_active(now) {
                outcome = ByToken::Expired;
                return Change::Keep;
            }
            outcome = ByToken::Done;
            Change::Write(Hold { created_ms: now, ttl_ms: Some(ttl_ms), ..hold.clone() })
        }
        _ => Change::Keep,
    })?;
    Ok(outcome)
}

/// What stands in the way of a lease, for the measurer to read: owner, build and expiry, never a token.
pub fn describe(current: &HoldRead) -> String {
    match current {
        HoldRead::Missing => "no hold".into(),
        HoldRead::Unreadable(why) => format!("an unreadable hold file ({why})"),
        HoldRead::Held(hold) => {
            let until = hold.ttl_ms.map_or("no expiry".to_string(), |ttl| format!("until {} ms", hold.created_ms.saturating_add(ttl)));
            match &hold.owner {
                HoldOwner::Operator => format!("an operator's hold: {} ({until})", hold.reason),
                HoldOwner::Measurer { who, build, .. } => format!("{who}'s lease measuring {build} ({until})"),
            }
        }
    }
}

/// Release the lease `token` names; any other hold is left as it is.
pub fn release(state_dir: &Path, token: uuid::Uuid) -> io::Result<ByToken> {
    let mut outcome = ByToken::NotYours;
    change(state_dir, |current| match current {
        HoldRead::Held(hold) if leased_by(hold, token) => {
            outcome = ByToken::Done;
            Change::Remove
        }
        _ => Change::Keep,
    })?;
    Ok(outcome)
}

fn leased_by(hold: &Hold, token: uuid::Uuid) -> bool {
    matches!(&hold.owner, HoldOwner::Measurer { token: held, .. } if *held == token)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Codex on 17660bf67f; Cormac on #6286): an older hold read as no hold and
    // overwritten, an older hold read as typed by a silent default instead of an explicit migration, an
    // unreadable file read as no hold, and a change applied to a hold that is not the one decided on.
    #[test]
    fn an_older_hold_holds_until_migrated_and_a_change_sees_what_it_replaces() {
        let state = tempfile::tempdir().expect("test: a state dir");
        assert_eq!(read(state.path()), HoldRead::Missing);
        // the M5's own file (2026-11-11): JSON without an owner holds, unreadable, until migrated
        let pre_owner = r#"{"reason": "canary 10523b95e: live read", "created_ms": 1794381896956, "ttl_ms": 7200000}"#;
        std::fs::write(state.path().join("deploy-hold"), pre_owner).expect("test: an older JSON hold");
        assert!(matches!(read(state.path()), HoldRead::Unreadable(_)), "no default reads an old file as typed");
        assert!(read(state.path()).honored().is_some(), "an unmigrated hold still holds");
        let migrated = match migrate(state.path()).expect("test: the migration") {
            Migrated::Rewritten(hold) => hold,
            other => panic!("an older JSON hold is rewritten, got {other:?}"),
        };
        assert_eq!((migrated.created_ms, migrated.ttl_ms, &migrated.owner), (1794381896956, Some(7200000), &HoldOwner::Operator));
        assert_eq!(migrate(state.path()).expect("test: again"), Migrated::Nothing, "the migration is idempotent");
        // bash's text hold: migrated as a TTL-less operator's hold
        std::fs::write(state.path().join("deploy-hold"), "operator: live read of #5917\n").expect("test: a legacy hold");
        let legacy = match migrate(state.path()).expect("test: the text migration") {
            Migrated::Rewritten(hold) => hold,
            other => panic!("a text hold is rewritten, got {other:?}"),
        };
        assert_eq!(read(state.path()), HoldRead::Held(legacy.clone()));
        std::fs::write(state.path().join("deploy-hold"), "{\"nonsense\": 1}").expect("test: a stranger");
        assert!(matches!(migrate(state.path()).expect("test: a refusal"), Migrated::Refused(_)), "a stranger's JSON is never rewritten");
        std::fs::write(state.path().join("deploy-hold"), serde_json::to_vec(&legacy).expect("test: encode")).expect("test: back to the typed hold");
        assert_eq!((legacy.reason.as_str(), legacy.ttl_ms), ("operator: live read of #5917", None));
        let seen = change(state.path(), |current| {
            assert_eq!(current, &HoldRead::Held(legacy.clone()), "the change decides on the hold as it is");
            Change::Keep
        })
        .expect("test: a kept hold");
        assert_eq!(seen, Change::Keep);
        let ours = Hold { reason: "ours".into(), created_ms: 5, ttl_ms: Some(10), owner: HoldOwner::Operator };
        change(state.path(), |_| Change::Write(ours.clone())).expect("test: a write");
        assert_eq!(read(state.path()), HoldRead::Held(ours));
        change(state.path(), |_| Change::Remove).expect("test: a release");
        assert_eq!(read(state.path()), HoldRead::Missing);
        assert!(HoldRead::Unreadable(Unreadable::Io { path: PathBuf::from("x"), kind: io::ErrorKind::PermissionDenied }).honored().is_some_and(|hold| hold.ttl_ms.is_none()), "an unreadable file holds");
    }

    // what this catches (Codex on #6288): a measurer releasing a newer run with a stale token, a lease
    // taken over an operator's hold or a live lease, and a lost measurer holding the node forever.
    #[test]
    fn a_lease_is_renewed_and_released_only_by_its_own_token() {
        let state = tempfile::tempdir().expect("test: a state dir");
        let ttl = 900_000;
        let first = match acquire(state.path(), &LeaseAsk { who: "codex", build: "4da0f81bfa", ttl_ms: ttl }, &|| 1_000).expect("test: acquire") {
            Acquired::Leased { token } => token,
            busy => panic!("a free node is leased, got {busy:?}"),
        };
        // a second acquisition while it is live is busy, even for the same build
        assert!(matches!(acquire(state.path(), &LeaseAsk { who: "codex", build: "4da0f81bfa", ttl_ms: ttl }, &|| 2_000).expect("test: again"), Acquired::Busy(_)));
        // renewed through readiness and the window's end, only with its token
        assert_eq!(renew(state.path(), first, ttl, &|| 500_000).expect("test: renew"), ByToken::Done);
        assert_eq!(renew(state.path(), uuid::Uuid::new_v4(), ttl, &|| 500_000).expect("test: a stranger"), ByToken::NotYours);
        // past its TTL with no successor: a late renewal never revives it, since the tracker may already
        // have moved on (Codex on #6288)
        assert_eq!(renew(state.path(), first, ttl, &|| 500_000 + ttl).expect("test: late"), ByToken::Expired);
        // lost measurer: past its TTL another run (an ancestor candidate) acquires, and the stale token
        // can neither renew nor release the newer run
        let second = match acquire(state.path(), &LeaseAsk { who: "codex", build: "f6648967d", ttl_ms: ttl }, &|| 500_000 + ttl).expect("test: after expiry") {
            Acquired::Leased { token } => token,
            busy => panic!("an expired lease frees the node, got {busy:?}"),
        };
        assert_eq!(release(state.path(), first).expect("test: stale release"), ByToken::NotYours);
        assert!(matches!(read(state.path()), HoldRead::Held(hold) if leased_by(&hold, second)), "the newer run still holds");
        assert_eq!(release(state.path(), second).expect("test: release"), ByToken::Done);
        assert_eq!(read(state.path()), HoldRead::Missing);
        // an operator's hold, active, makes a lease busy and is never released by one
        let operator = Hold { reason: "operator: live read".into(), created_ms: 1_000, ttl_ms: None, owner: HoldOwner::Operator };
        change(state.path(), |_| Change::Write(operator.clone())).expect("test: an operator hold");
        assert!(matches!(acquire(state.path(), &LeaseAsk { who: "codex", build: "x", ttl_ms: ttl }, &|| 2_000).expect("test: busy"), Acquired::Busy(HoldRead::Held(_))));
        assert_eq!(release(state.path(), second).expect("test: not the operator's"), ByToken::NotYours);
        assert_eq!(read(state.path()), HoldRead::Held(operator));
    }
}
