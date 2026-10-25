#![cfg(test)]
use super::*;

// what this catches: the faculty ABSTAINS with no recorded reasoning (first
// turns / suppressed thinking) — it never bids empty noise into the workspace.
#[tokio::test]
async fn faculty_abstains_when_empty() {
    let wm = Arc::new(WorkingMemory::new(3));
    let faculty = WorkingMemoryFaculty::new(wm);
    let ws = Workspace::new("a fresh burst");
    assert!(faculty.contribute(&ws).await.is_none());
}

// what this catches: once reasoning is recorded, the faculty bids it as
// context at the working-memory salience tier — the reasoning the deliberator
// will see next tick. Newest is rendered LAST (positional recency).
#[tokio::test]
async fn faculty_bids_recent_reasoning_as_context() {
    let wm = Arc::new(WorkingMemory::new(3));
    wm.record("I should check the room roster first.");
    wm.record("Now I will read the latest message.");
    let faculty = WorkingMemoryFaculty::new(Arc::clone(&wm));
    let ws = Workspace::new("a fresh burst");
    let c = faculty.contribute(&ws).await.expect("bids when non-empty");
    assert_eq!(c.faculty, FacultyId::Custom("working-memory".to_string()));
    assert!((c.salience - WORKING_MEMORY_SALIENCE).abs() < f32::EPSILON);
    assert!(c.content.contains("check the room roster"));
    // newest trace is LAST (position carries recency, no relative label).
    let oldest_at = c.content.find("room roster").unwrap();
    let newest_at = c.content.find("latest message").unwrap();
    assert!(newest_at > oldest_at, "newest trace rendered last");
    assert!(c.decision.is_none(), "perception bid carries no decision");
}

// what this catches: the KV-cache-locality invariant this formatter exists to
// hold — appending a new trace must NOT rewrite the rendering of prior traces.
// (The old "(turn -N)" relative label broke this: every prior entry's offset
// shifted on each append, mutating the whole block from its first byte and
// forcing a full re-prefill of the dynamic tail every settle-act.)
#[tokio::test]
async fn prior_traces_render_byte_identical_after_append() {
    let wm = Arc::new(WorkingMemory::new(8));
    wm.record("first");
    wm.record("second");
    let faculty = WorkingMemoryFaculty::new(Arc::clone(&wm));
    let ws = Workspace::new("burst");
    let before = faculty.contribute(&ws).await.unwrap().content;
    wm.record("third");
    let after = faculty.contribute(&ws).await.unwrap().content;
    // the older render is a strict PREFIX of the newer one — pure append.
    assert!(
        after.starts_with(&before),
        "appending a trace must leave prior traces byte-identical (cacheable prefix);\n  before={before:?}\n  after={after:?}"
    );
}

// what this catches: #205 — the working-memory faculty's contribution is marked
// TRAILING, so the deliberation faculty renders it nearest generation as a
// conversation turn (not in the system message). That is what keeps the cacheable
// system prefix byte-stable across a settle-act instead of re-prefilling the whole
// tail. The proprioceptive content (its own thinking + the [action #n] receipt
// trail) is unchanged — only WHERE it lands moved. regression for #205
#[tokio::test]
async fn working_memory_contribution_is_marked_trailing() {
    let wm = Arc::new(WorkingMemory::new(8));
    wm.record("I should write the login form first, then style it.");
    wm.record_receipt("I ran code/write({\"file_path\":\"login.html\"}) Result: ok, 812 bytes");
    let faculty = WorkingMemoryFaculty::new(Arc::clone(&wm));
    let c = faculty
        .contribute(&Workspace::new("build a login form"))
        .await
        .expect("recent content → a contribution");
    assert!(
        c.trailing,
        "working-memory proprioception must render nearest generation, not in the system prefix (#205)"
    );
    // Content is unchanged by the move — the mind still perceives its own hands.
    assert!(
        c.content.contains("write the login form first"),
        "the reasoning trail survives: {}",
        c.content
    );
    assert!(
        c.content.contains("[action #1]"),
        "the action receipt survives — this faculty IS the proprioception channel: {}",
        c.content
    );
}
