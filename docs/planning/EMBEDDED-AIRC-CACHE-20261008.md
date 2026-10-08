# Embedded projection-cache isolation — 2026-10-08

Card af68177f-d3c6-4f50-8cad-9a745af3536e. Owner: Codex retirement_review.

AIRC PR1560 merged as ee497fa1e701c453924aa5a89867bd920b63270e. Its shared projection-cache owner namespaces board and wall snapshots by format version and source; older readers retain their own paths instead of overwriting another reader's snapshot. Legacy files remain intact and are rebuilt once into the new namespace. This addresses the repeated cold replay mechanism diagnosed during card-read latency work, without claiming every slow read came from that mechanism.

Continuum previously embedded db7ac8d5. Updating the standalone AIRC installation alone does not update its embedded library. This change moves all eight workspace pins and fifteen lockfile source entries to ee497fa1; no registry dependency versions or edges change. No second cache implementation is introduced in Continuum.

The upstream owner reported 9 cache unit tests, 5 board integration tests, 1 wall fixture and 1 signed-review replay fixture green, plus strict workspace Clippy. Local full cargo metadata --locked passed. The existing Continuum work/get consumer fixture passed (1 test, 0 failed, 0.56s execution after 7m33 compilation) against the new pin, retaining subscribed-card, focus and signed-review consumer coverage. Independent source review approved b2442228 in PR4866 comment6068329401. CI remains pending.

No runtime install, cache deletion, source fallback build, or persona intervention was performed. Windows recovery and the portable CPU artifact repair remain separate prerequisites for safe adoption. Actual installed embedded revision and consumer continuity must be verified before calling this delivered.