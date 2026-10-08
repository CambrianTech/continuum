# Explicit prebuilt embedding validation

Card 9ccffc6e-cde8-4597-b839-88e4afc54113 addresses the validation gap exposed by
the Windows 9f11 illegal-instruction crash. A build SHA can be printed without
executing the native instructions needed to initialize an embedding context.
The portable CPU build repair remains separate (card 8a3f42ec).

Use the existing prebuilt validator with an explicit, local retrieval GGUF:

```
continuum reboot --prebuilt <candidate-core> --validate-only --embedding-model <local-embedding.gguf>
```

This first performs the existing provenance check. It negotiates embedding probe
support through `--build-sha --validation-capabilities`: an old core safely
returns its ordinary SHA and is refused. It must never receive an unfamiliar
model flag, which older cores could mistake for a socket path and start serving.
The advertised revision must match the prepared candidate and protocol must be 1.

Only then does the validator start the candidate's early-exit model probe. It
loads the specified model on CPU and uses the **same embedding implementation**
as the production llama backend: context creation, tokenization, last-token
pooling, decode and nondegenerate finite normalized vector validation. The
probe returns dimensions and load/embed timings, never vectors or user content.
Use the actual configured retrieval embedder (currently Qwen3-Embedding), not a
different model chosen just to pass. This proves native execution for that model,
not semantic retrieval quality, persona learning or general grid health.

Candidate dispatch happens before config loading, Tokio runtime creation,
tracing persistence, sockets, personas or activity startup. The model and context
use existing RAII cleanup. The parent imposes a 120-second model deadline and
kill-on-drop cleanup; crash/nonzero exit, timeout, absent capability, wrong
revision, malformed receipt and empty vector all refuse validation. The existing
SHA probe uses the same subprocess owner with its existing 30-second deadline.
Ordinary validation and installation do not implicitly run a model. The explicit
model option requires validate-only and cannot perform a service handoff.

The lightweight lifecycle fixture covers success/failure, timeout cleanup,
invalid receipts and rejection of an old candidate before any model probe.
The full lifecycle suite passed (68 tests, 10.84 seconds), including the actual
safe capability argv order. Strict lifecycle Clippy passed. Two existing parser
idioms were corrected in separate commit `815828d55`; no warning suppression or
baseline increase was used. Both core-server and CLI passed `cargo check`
(4 minutes 6 seconds, existing warnings). Existing CLI option scenarios were
extended for the explicit opt-in contract. Their local execution was blocked
before running by the existing Windows core cdylib link limit: `LNK1140: limit
exceeded for program database`. No cache was cleared or checks bypassed. The
fixture must still execute successfully in CI or a supported build environment.
Independent source review approved the corrected capability negotiation;
final exact-commit review and CI remain pending.
No candidate has been run against a real model yet. No production service,
persona, cache, model file or credential was altered to validate this source.

Installed acceptance remains open: after a corrected artifact is published,
run this explicit validation on the non-AVX512 Windows host with the actual
retrieval model, record candidate SHA, dimensions, timings and clean child exit,
then use the normal guarded deployment path. Keep any crash dump private.
