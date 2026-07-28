# legacy/ — quarantined, dead code. NOT in any build, test, or CI path.

This directory holds **retired** code kept only for reference while its
replacement settles. Nothing here is compiled, tested, packaged, or shipped:

- It is **not** a Cargo workspace member (see root `Cargo.toml` — members are
  explicit; `legacy/` is absent).
- It is **not** referenced by any `npm` script, `Dockerfile`, or CI workflow.
- Do **not** edit, fix, or wire anything here. If you find yourself editing a
  file in `legacy/`, stop — you're patching poison. Fix the replacement instead.

The rule (Joel): move legacy *out of the path first*, let the build/refs blow up,
and chase the breakage to the pure replacement — never work around the old thing.

## node-startup/

`parallel-start.sh` — the legacy Node start orchestrator that ran behind
`npm start`. Slow, single-threaded, and it broke on stale paths (`cd workers`
after the workers→core rename; missing `@gltf-transform/core` scene-gen) that had
nothing to do with the substrate.

**Replacement:** `tools/scripts/start-server.sh` — the pure-Rust headless start
(`cargo run` the `continuum-core-server` directly, per-platform GPU features, no
Node). Both root and `src/` `package.json` `start` now point at it. Talk to the
running core with the Rust **`continuum`** client (`continuum ping`, …) — the replacement for
the Node `./jtag`.
