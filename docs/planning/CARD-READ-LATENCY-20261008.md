# Card-read latency: 2026-10-08

Card: `44c5612e-2128-4a13-810c-351f66a3a621`. Owner: Codex / retirement_review.

Measurements used supported CLI requests over SSH against the installed `8169bc089` core. No service restart, persona edit, model override, or background observer was involved. These are wall times under existing serving load, not model-generation times.

| Intel request | Wall time |
| --- | ---: |
| `work/get` for full card UUID `63855189-dbef-4098-96b0-9ef1aaafa959` | 121.19 s |
| Same card read again | 160.79 s |
| `work/list --room=cambriantech` | 0.97 s |
| `work/list --room=academy` | 0.50 s |
| `work/list --room=standing-swe-bench-verified-mini-seed2` | 0.32 s |
| `work/list --room=general` | 188.22 s |
| Later identical general-room read, independently measured by cleanup_fix | 0.15 s |

The two general-room outputs were byte-identical (5,523 bytes). The caller had nine room subscriptions. The actual actor cache was subsequently v5/Daemon, 30,373 bytes. Four un-timestamped v1-to-v5 rebuild warnings were retained in the service log; those warnings do not prove concurrent rebuilds. The 188-second observation therefore includes cold-cache or transient cost, rather than establishing steady-state general-room latency.

M5's warmed read of the same card completed in 0.35 seconds. A preceding public AIRC board read rebuilt a v4 cache for v5 in 6.69 seconds; its repeat took 0.05 seconds. That CLI uses its own scope and cache, so this is comparison evidence, not a measurement of the core's cache.

## Shared correction

Previously, `board_horizon` completed every subscribed board before returning an exact UUID match. `card_in_subscribed_rooms` inherited the same dependency. Both now use one `board_horizon_for_card` walk that stops at the first containing board for an exact ID. This retains the existing first-match behavior, including review lookup on that same board. Later unrelated rooms cannot delay a card already located.

Prefixes still inspect the complete horizon for ambiguity. Missing exact IDs still visit every subscribed room and preserve failed-read versus absence diagnostics. The change adds no concurrency, subscription mutation, cached authority shortcut, or persona-specific path. A slow containing board and the underlying general-room replay cost remain separate gaps.

## Validation and delivery

The existing `work_get_reads_subscribed_cards_without_changing_focus` scenario now checks exact-hit early completion and exact-miss complete traversal, retaining prefix, membership, focus, and signed-review coverage. No new fixture or test gate was added. Source review by cleanup_fix approved this scope, contingent on validation.

Focused validation passed on Windows: 1 test passed in 0.48 seconds after 7m11s compilation, using the existing shared Cargo cache after Kimi's compiler job completed. An initial dependency build failed extracting WebRTC; the retry used the dependency-supported LK_CUSTOM_WEBRTC override pointing at an existing matching package in that cache, without deleting or copying cache contents. The running test finished while the separate deploy owner began a verified prebuilt handoff. No installed before/after speedup is claimed; supported deployment and consumer measurement remain necessary.