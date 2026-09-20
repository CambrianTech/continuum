# legacy/persona-exercises — the first code personas wrote

These are throwaway practice crates authored by resident personas during the
July 2026 "prove the hands work" arc, not part of the Continuum substrate.
Nothing in the build, the tests, or the runtime references them.

| dir | what it is |
|---|---|
| `string_reversal/` | reverse-a-string exercise, as a crate |
| `wordstats/` | word-frequency counter |
| `work-wordstats/` | the same task run against a work card, with its `sample.txt` |
| `teamproof/` | eight one-file algorithm exercises (atoi, LRU cache, RLE roundtrip, spiral order, …) from a two-solver run |
| `loose-files/` | single files a persona wrote straight into the repo root — `main.rs`, `life.rs`, `game_of_life.rs`, `reverse.rs`, `string_reversal.rs`, plus the `sample.txt` / `simple_text.txt` / `test_simple.txt` inputs they read |

`loose-files/` is the clearest illustration of the defect below: a bare
`main.rs` sitting beside `Cargo.toml` at the root of a Rust workspace, which is
what anyone cloning the repo saw first.

## Why they are kept, and why they were in the repo root

They are kept because they are the **first genuine persona-written code** in
this project — the evidence that the act→observe circuit closed and a citizen
actually compiled and ran something. That is worth having.

They were in the repo *root* because of a defect, not a decision: until #49
(per-persona workspace isolation) lands, a persona's file engine roots at the
process CWD, which is the repo root. So her card work landed next to
`Cargo.toml` and got committed alongside real changes.

The fix is #49 — give each persona a real workspace so her output is
deliberate and reviewable, the same as any other contributor's. Personas are
expected to build Continuum itself; the answer is never to hide where they
write, it is to give them somewhere proper to write.

## Do not extend this directory

New persona work does not belong here. If a persona produces something worth
keeping, it belongs in the tree on its own merits, via a normal commit.
