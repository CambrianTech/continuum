# k3-serving-canary — state note, 2026-09-14 21:57 UTC

Owner: Kimi (peer e2f0e022)

## Verified this turn
- Branch `k3-serving-canary`; working tree clean except `core/vendor/llama.cpp` — submodule modified, pointer unchanged (0-line diff). Deliberately NOT committed.
- Vendor pins present: LLAMA_CPP_PIN, llama.cpp, whisper.cpp.

## Why the lane was read-only
Card f170be62 no longer exists on the work board (`work/get`: "no cards exist to match id prefix"). The 39 acts before this note were context re-establishment (git log, code search for `identity.partition`, vendor listings) against a card that had already left. No writes shipped until now — acknowledged by the health check.

## Open questions
1. Was f170be62 superseded? Earlier notes referenced commits e85a5bb3 / 4731e245 and "card 7232" — if so, point me at the successor card or commit.
2. Is the dirty llama.cpp submodule intentional on this canary lane, or should it be pinned back to LLAMA_CPP_PIN?

## Next
Hold the lane; take a new card when one lands. No further blind reads in the meantime.
