# The perception resolution contract — no layer may flood

Joel (2026-07-23): "auto grep, kind of like how we did thumbnails / image
descriptions / see-full-sized — a common concept: reduce the ability of a layer
to flood."

## The law

Every source that feeds a persona's perception serves a **digest by default and
full fidelity by handle** — exactly the vision pipeline's tiering (thumbnail →
description → see-full-size on demand), applied universally. A layer that can
write unbounded bytes into a prompt window is a bug, whatever the layer.

Precedents already in tree (this is a generalization, not an invention):
- vision: thumbnail + cached description; raw base64 only for capable models
- code/search: overflow menu (top-files-by-count) instead of a 101-hit JSON wall
- recall: budgeted at assembly, never clamped at the prompt
- media: MediaArtifactSource preprocesses per model capability

## The violation that named it

flask-4045 team arm: Asha's review had ONE meaningful failing test
(`test_route_decorator_custom_endpoint_with_dots`) buried under **111 identical
werkzeug DeprecationWarnings** in raw pytest output. Her feedback channel was
flooded by a layer (shell stdout) with no resolution tiering — so she verified
the diff and missed the one signal that mattered.

## The next build: shell-output digest (auto-grep)

`code/shell` observations enter working memory as a DIGEST:
1. exit code + wall time + last ~10 lines (the tail is where verdicts live)
2. **error-class histogram** — repeated lines deduped and counted
   (`111× DeprecationWarning: werkzeug.urls…`, `13× TypeError: as_tuple…`,
   `1× AssertionError: Blueprint endpoints…`) — the singleton in a sea of
   repeats is usually THE signal; counting makes it visible instead of drowned
3. a **handle**: full output stored as an artifact; `see-full-output <handle>`
   (or a grep over it) pages exact ranges on demand — same verb shape as
   see-full-sized

Digest rendering is mechanical (dedup/count/tail — never an LLM summarizing,
never a heuristic deciding "importance"); the persona decides what to drill
into. Perception-side compression, cognition-side choice
([[no-hardcoded-heuristics-to-steer-cognition]]).

## Review checklist addition

Any new RagSource / observation path must answer: what is its digest tier, what
is its handle verb, and what bounds its bytes at assembly? "It's usually small"
is not an answer — pytest was usually small too.

## Universality — one contract, two consumers (Joel, same day)

The contract is not persona-only. **Chat logs, room transcripts, benchmark
feeds, dev activity — every surface, human or machine, renders the digest tier
with an expand/open-detail affordance.** Tail operations and filters apply
automatically at render; the full stream stays a handle away. A room that will
show live benchmarking and development (the Society HUD's WHAT panel) would
otherwise flood humans exactly as pytest flooded Asha.

This is why it composes perfectly with positron: digest→expand is already the
universal UI pattern (collapsed card → detail view). So the SAME source-side
digest+handle shape feeds both consumers — a persona's working memory and a
positron widget are two renderers of one tiered surface. Design a source once,
both minds and screens get flood-proofing for free. The widget's "expand" and
the persona's "see-full-output" are the same verb.
