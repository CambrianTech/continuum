# Run ledgers are typed artifacts with one owner

**Status:** design of record, 2026-08-18. Written after three drift bugs in one afternoon,
all of them the same bug.

**The one-line contract:** a run's on-disk state is a TYPE, and exactly one component
resolves its directory, its filename, its field names and its state vocabulary. No caller
builds a path or a JSON key.

---

## The measured class

`~/.continuum/progress/` holds every long-running job's state. Today:

| Fact | Count |
|---|---|
| Ledger families (`agent-solve-`, `models-pull-`, `competition-`, eval, teach, fitness-sentinel) | 6 |
| Independent spellings of the progress directory | 5 |
| `json!({…})` literals writing records | ~29 |
| Stringly-typed `get("state")` reads | 13 |
| Shared types across families | **0** |

Every family hand-rolls the same six decisions: where the directory is, what the file is
called, what the fields are called, what the states are called, how to enumerate, and how
to tell a record from its sibling (`X.grade.json`). Six decisions × six families = 36
independent chances to disagree, and nothing forces agreement.

### The three bugs today were three draws from that surface

1. **Filename.** `agent/solve` wrote `agent-solve-<run>.json`; the boot reaper and the
   reboot guard read `swe-solve-*`, which nothing has ever written. 19 orphans frozen as
   `running`, oldest 162 h. The reboot guard reported "nothing in flight" while 19 runs
   said otherwise.
2. **Directory.** `solve_ledger_dir()` ignored `CONTINUUM_HOME`; the writer honoured it.
   A second spelling of the root.
3. **Field.** My own fix inserted `runId` into a family that spells it `run_id`. Two names
   for one field, in the commit that collapsed two names for one file.

Same shape as `swe_cache_dir`'s documented history (two env roots, "77% have no
environment" vs the real 95%). This is not a benchmark problem. It is what an untyped
artifact does.

---

## Why this class is uniquely brittle: the failure is an EMPTY READ

A wrong path does not raise. `read_dir` succeeds, the filter matches nothing, the function
returns `Vec::new()`, and the caller cannot distinguish that from *"nothing to do."*

That is the amplifier. A guard that returns empty **reports safety**. The reboot guard
was, for weeks, a green light that had never once looked at a real run. Deterministic —
same answer every time — and blind.

> **Determinism over the wrong thing is worthless.** Reliability requires that the reader
> and the writer be the same decision, not two decisions that happen to agree.

---

## The design

Four layers. Reuses the house `*CaptureSink` idiom (7 existing members) rather than
inventing a new shape.

### L1 — the record is a type

```rust
/// One family's on-disk record. Serde owns the field names; the enum owns the vocabulary.
pub trait RunRecord: Serialize + DeserializeOwned + Send + Sync {
    /// Filename prefix, e.g. "agent-solve-". The ONE place it is spelled.
    const PREFIX: &'static str;
    fn state(&self) -> RunState;
    fn set_failed(&mut self, cause: &str);
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunState { Running, Failed, Complete }
```

Field drift becomes a **compile error**. State drift becomes a **parse error**. Neither is
reachable by writing a different string literal in a different file.

### L2 — one owner of the artifact

```rust
pub struct RunLedger<R: RunRecord> { dir: PathBuf, _r: PhantomData<R> }

impl<R: RunRecord> RunLedger<R> {
    pub fn open() -> Self;                       // resolves the root ONCE, honours CONTINUUM_HOME
    pub fn write(&self, id: &RunId, rec: &R) -> io::Result<()>;
    pub fn read(&self, id: &RunId) -> io::Result<Option<R>>;
    pub fn iter(&self) -> impl Iterator<Item = (RunId, R)>;   // skips siblings, never phantoms
    pub fn amend(&self, id: &RunId, f: impl FnOnce(&mut R)) -> io::Result<()>;  // merge, never clobber
    pub fn sibling(&self, id: &RunId, kind: SiblingKind) -> PathBuf;  // .grade.json etc
}
```

No caller builds a path. No caller writes a JSON key. `amend` exists because the reap's
blanket overwrite destroyed the `workspace` pointer — the only thing that could locate the
artifact. **An annotation is never a replacement.**

### L3 — reconciliation is a trait, not per-family code

```rust
pub trait Reconcile { fn reap_orphans(&self) -> Vec<RunId>; }
```

Default impl over `RunLedger<R>`: enumerate, every `Running` becomes `Failed` with a cause,
payload preserved, idempotent. This is Joel's rule in code — deterministic, no scan, no
poll, same answer given the same state. Every family inherits it instead of writing it
(today only one family has it, and it was pointed at nothing).

### L3b — the record carries PROGRESS, because liveness is not progress

Added 2026-08-18 after the finding below, which is the same defect one level up from the
ones that motivated this doc.

An ad-hoc grading job ran for ~10 hours and produced **nothing**: worklist item 1's work
tree was last touched at 10:15, checked at 20:06, one file, zero output, zero verdicts. It
was reported as healthy six times on the strength of `pgrep` returning a pid. A process
existing is not a process working — the same error as health-from-connection-existence
rather than acked delivery, and as an empty read reported as "nothing to do".

So `RunRecord` must carry, and `RunLedger` must expose:

```rust
fn heartbeat_ms(&self) -> u64;   // last time the run PROVED progress, not last time it existed
fn deadline_ms(&self) -> Option<u64>;
```

with a derived `is_wedged(now)` — heartbeat older than its own declared cadence. Then
"wedged" is a state the reconciler can settle deterministically, exactly like an orphan,
instead of a thing a human has to notice. The wedge class already has history here (#385
inference await blocking its thread, #386 wedge-killed attempts burning as capability
zeros); the missing piece was never detection logic, it was a record with a heartbeat in it.

**Corollary, and it is the reason this belongs in the architecture and not in a runbook:**
the reboot guard fixed this morning protects exactly the work that *registers itself in a
ledger*. My grading job registered nowhere, so it was invisible to the guard by
construction — not a bug in the guard, a consequence of living outside the type. Ad-hoc
operator jobs are outside every safety mechanism the substrate has. Either they enter
through a ledger or they get no guard, no wedge detection, no reap, and no receipt. That is
an argument for making the registered path the *easy* path, since the ad-hoc one will
otherwise keep being chosen under time pressure — by me, demonstrably.

### L4 — the anti-blindness law (this is the part that actually prevents recurrence)

**A test may not hand-author the artifact it reads.** Fixtures come from the production
writer.

Today's reaper test wrote `swe-solve-alive.json` by hand and asserted an
`agent-solve-other.json` was "another subsystem's ledger, untouched." It tested the reader
against *the test author's belief about the writer*, so it stayed green while the live path
was dead — and it actively documented the bug as intent.

```rust
// The round-trip test every family gets for free.
#[test]
fn what_the_writer_writes_is_what_the_reader_finds() {
    let led = RunLedger::<SolveRun>::open_in(tmp.path());
    led.write(&id, &SolveRun::running(&instance, &workspace));   // PRODUCTION writer
    assert_eq!(led.iter().count(), 1, "the reader finds what the writer wrote");
    assert_eq!(led.reap_orphans(), vec![id.clone()]);
    let after = led.read(&id).unwrap().unwrap();
    assert_eq!(after.state(), RunState::Failed);
    assert_eq!(after.workspace, workspace, "an annotation never erases the payload");
}
```

This single test, existing for any family, makes bugs 1–3 unrepresentable.

---

## Build order (outlier validation, per the methodical process)

1. **L1+L2 against outlier A — `agent-solve`.** Simplest family: one file per run. Already
   half-collapsed by `f3cb3a65c`, so this is finishing a move in flight.
2. **Outlier B — `cognition/eval`.** Deliberately the MOST different: 11 `json!` literals,
   separate progress/result/status records, its own `runId` camelCase spelling, live
   polling by `observe`. If the interface fits solve *and* eval without forcing, it fits
   the middle four. **If it does not fit, redesign before migrating anything else** — that
   is the point of picking the extremes.
3. **L3 + L4 on both.** Reap and the round-trip test.
4. **Generator.** `ledger/new <family>` scaffolds record + store binding + the round-trip
   test, so family seven inherits all of this instead of hand-rolling it. Generators encode
   the pattern; docs only describe it.
5. **CI guard**, same idiom as `no_new_hardcoded_context_or_prompt_size_constant…`: fail on
   a new `join("progress")` outside `RunLedger`, and on a `json!({` literal written into
   the progress root. The guard is what stops it regrowing after the migration.
6. **Migrate the remaining four**, one per PR.

Steps 1–3 are the slice that pays for itself; 4–5 are what make it permanent. Do not skip 5
— the tree already proves prose does not hold a convention.

---

## Acceptance

- Grep the tree: exactly ONE `join("progress")`, ONE spelling per family prefix, ZERO
  `json!` literals writing a run record.
- Rename a field in a record struct → every reader fails to COMPILE, none fails silently.
- Point a reader at a family with no records → still empty, but no reader CAN be pointed at
  the wrong family, because the family is a type parameter.
- Kill a run mid-flight, reboot → verdict exists, payload intact, second reboot changes
  nothing.

## What this does NOT do

It does not make the grade tail fire. The 13 ungraded artifacts have no cards at all
(#425) — a separate gap. This removes the *drift* class beneath the benchmark work; it is
plumbing, and it should be judged as plumbing.
