# Testing `continuum-core`

## TL;DR — use the wrapper

```bash
# From `src/`:
./scripts/cargo-test.sh tick_db_handle --lib
./scripts/cargo-test.sh --test no_cpu_fallback_contract
./scripts/cargo-test.sh --lib -- --test-threads=1

# Or via npm:
npm run test:rust -- tick_db_handle --lib
```

The wrapper sources `scripts/shared/cargo-features.sh` to apply the
right GPU feature flags for the current platform automatically.

## Why a wrapper?

The vendored `llama` crate intentionally requires `--features metal`
(macOS) or `--features cuda` / `--features vulkan` (Linux) so the
build refuses to produce a CPU-only inference binary — see the
no-CPU-fallback alpha contract (`tests/no_cpu_fallback_contract.rs`,
issue #1262).

That guard is correct, but it makes the obvious developer command
fail before the test runs:

```bash
cd workers/continuum-core && cargo test tick_db_handle --lib
# → fails in the llama crate; "metal" or "cuda" feature required
```

Manually adding the right features per platform is repetitive and
brittle (fresh installs, agents, and new contributors all hit it
once before learning the incantation):

```bash
# macOS:
cargo test tick_db_handle --lib --features metal,accelerate
# Linux + Nvidia:
cargo test tick_db_handle --lib --features cuda,load-dynamic-ort
# Linux + AMD:
cargo test tick_db_handle --lib --features vulkan,load-dynamic-ort
# …
```

`scripts/cargo-test.sh` reuses the same `cargo-features.sh` detector
that `git-prepush.sh` and `build-with-loud-failure.sh` already
source, so there's only one place that knows the platform→features
mapping.

## CPU-only debug mode (advanced)

To deliberately reproduce the no-features failure (e.g. when
verifying the loud-fail guard itself):

```bash
CARGO_TEST_NO_FEATURES=1 ./scripts/cargo-test.sh --lib
# macOS: fails in llama crate (expected — that IS the contract)
# Linux: succeeds for non-inference tests (no llama feature gates)
```

This does NOT weaken the compile-time guard; it just lets you see
what the bare command does without auto-applying features.

## Targeting a different workspace package

```bash
CARGO_TEST_RUST_PACKAGE=inference-grpc ./scripts/cargo-test.sh --lib
```

Defaults to `continuum-core`.

## How this fits with the rest of the test infra

| Command | When | Notes |
|---|---|---|
| `npm run test:rust ...` | iterative dev | Uses this wrapper, fastest feedback |
| `npm run test:precommit` | before commit | Wider scope (TS + browser ping) |
| `npm run test:prepush` | before push | Includes Rust + native Docker checks |
| `cargo test ... --features metal,accelerate` | one-off, raw | Skips the wrapper; useful for debugging |

Per #1257 (the card that motivated this), the wrapper is the
documented default; the raw form remains available for cases where
you want to override feature selection explicitly.
