# Shape ratchets: per-crate baseline (2026-11-14)

The receipt for the six shape ratchets of CONTINUUM-ENGINE.md, RTOS rules 10-11
(`core/source-hygiene/src/shapes/`). Measured by CI (Source Hygiene Ratchets, run 43547453878,
PR #5990), with the PR's rules, over canary `958ea3056` (the base run). Each cell is
`production / tests`. Nothing here is judged: the gate compares every PR's totals with its own
base tree (`src/ratchet.rs`), and every run prints this table again, folded in its log under
"every crate of every ratchet". This file is the starting line a floor-up sweep (plan phase 5)
reads; it goes stale the day it lands, and that is the point.

Crates are ordered by total sites, largest first. A crate absent from the table has none.

| Crate | waits | unknown | stringly | flags | string ids | contract | total |
|---|---|---|---|---|---|---|---|
| continuum-core | 118 / 60 | 10 / 6 | 376 / 52 | 15 / 2 | 170 / 40 | 340 / 292 | 1481 |
| continuum-engine | 70 / 84 | 35 / 9 | 613 / 30 | 26 / 5 | 115 / 45 | 199 / 213 | 1444 |
| continuum-mind | 41 / 13 | 9 / 1 | 271 / 37 | 27 / 0 | 63 / 20 | 367 / 135 | 984 |
| continuum-kernel | 45 / 45 | 13 / 0 | 301 / 49 | 6 / 1 | 65 / 17 | 206 / 87 | 835 |
| continuum-cognition | 8 / 19 | 52 / 16 | 88 / 16 | 26 / 1 | 35 / 7 | 252 / 206 | 726 |
| continuum-live | 12 / 6 | 4 / 0 | 139 / 5 | 5 / 0 | 72 / 0 | 174 / 65 | 482 |
| continuum-academy | 27 / 1 | 2 / 0 | 155 / 1 | 5 / 0 | 42 / 15 | 74 / 15 | 337 |
| continuum-dreaming | 4 / 6 | 1 / 0 | 166 / 3 | 5 / 0 | 33 / 2 | 47 / 42 | 309 |
| continuum-cli-lifecycle | 6 / 8 | 0 / 0 | 210 / 6 | 4 / 0 | 5 / 0 | 14 / 9 | 262 |
| continuum-grid | 7 / 1 | 6 / 0 | 74 / 0 | 3 / 0 | 60 / 9 | 37 / 14 | 211 |
| continuum-genome | 3 / 2 | 11 / 0 | 78 / 1 | 1 / 0 | 29 / 0 | 45 / 28 | 198 |
| continuum-hands | 7 / 12 | 1 / 0 | 86 / 0 | 5 / 0 | 14 / 0 | 46 / 14 | 185 |
| continuum-sentinel | 10 / 6 | 5 / 0 | 75 / 0 | 0 / 0 | 41 / 6 | 40 / 0 | 183 |
| continuum-serving | 9 / 8 | 12 / 0 | 8 / 0 | 0 / 0 | 11 / 0 | 59 / 44 | 151 |
| continuum-store | 0 / 0 | 1 / 0 | 60 / 25 | 0 / 0 | 6 / 0 | 32 / 3 | 127 |
| continuum-experience | 0 / 1 | 0 / 0 | 55 / 4 | 2 / 0 | 15 / 6 | 25 / 14 | 122 |
| continuum-context | 1 / 0 | 26 / 2 | 0 / 0 | 3 / 0 | 2 / 0 | 60 / 3 | 97 |
| continuum-positron | 1 / 2 | 0 / 0 | 9 / 6 | 1 / 0 | 9 / 10 | 10 / 18 | 66 |
| tool-working-set | 0 / 0 | 4 / 0 | 12 / 0 | 0 / 0 | 13 / 0 | 19 / 2 | 50 |
| continuum-vitals | 8 / 1 | 3 / 0 | 0 / 0 | 1 / 0 | 0 / 0 | 2 / 25 | 40 |
| llama | 0 / 0 | 0 / 0 | 26 / 0 | 1 / 0 | 0 / 0 | 7 / 1 | 35 |
| continuum-memory | 0 / 2 | 0 / 0 | 1 / 0 | 0 / 0 | 5 / 0 | 24 / 1 | 33 |
| continuum-gateway | 0 / 3 | 3 / 0 | 21 / 0 | 0 / 0 | 1 / 0 | 3 / 1 | 32 |
| serving-replay | 2 / 0 | 3 / 0 | 16 / 0 | 2 / 0 | 1 / 0 | 7 / 0 | 31 |
| continuum-capture | 0 / 0 | 2 / 0 | 4 / 0 | 0 / 0 | 14 / 0 | 10 / 0 | 30 |
| livekit-bridge | 0 / 0 | 0 / 0 | 17 / 0 | 0 / 0 | 1 / 0 | 8 / 0 | 26 |
| continuum-process | 4 / 4 | 0 / 0 | 5 / 0 | 0 / 0 | 0 / 0 | 7 / 5 | 25 |
| continuum-perception | 0 / 0 | 0 / 0 | 5 / 1 | 1 / 0 | 4 / 7 | 3 / 3 | 24 |
| continuum-airc-protocol | 1 / 0 | 0 / 0 | 12 / 0 | 0 / 0 | 0 / 0 | 8 / 0 | 21 |
| continuum-base | 0 / 0 | 0 / 0 | 15 / 0 | 0 / 0 | 1 / 0 | 0 / 0 | 16 |
| continuum-models | 2 / 0 | 0 / 0 | 2 / 0 | 0 / 0 | 0 / 0 | 11 / 1 | 16 |
| source-hygiene | 0 / 0 | 2 / 0 | 6 / 0 | 0 / 0 | 1 / 0 | 7 / 0 | 16 |
| continuum-service | 0 / 0 | 1 / 0 | 0 / 0 | 0 / 0 | 1 / 0 | 11 / 1 | 14 |
| continuum-saved-state | 0 / 0 | 1 / 0 | 0 / 0 | 0 / 0 | 5 / 0 | 5 / 2 | 13 |
| expert-pager-policy | 2 / 0 | 1 / 0 | 3 / 0 | 0 / 0 | 0 / 0 | 7 / 0 | 13 |
| continuum-grounding | 0 / 2 | 0 / 0 | 0 / 0 | 1 / 0 | 0 / 0 | 6 / 3 | 12 |
| continuum-rooms | 2 / 0 | 0 / 0 | 2 / 0 | 1 / 0 | 0 / 0 | 6 / 1 | 12 |
| continuum-host | 5 / 2 | 2 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 1 / 0 | 10 |
| continuum-engram | 0 / 1 | 0 / 0 | 4 / 0 | 0 / 0 | 0 / 0 | 4 / 0 | 9 |
| continuum-prompt | 0 / 0 | 6 / 0 | 0 / 0 | 1 / 0 | 0 / 0 | 1 / 0 | 8 |
| continuum-embedding | 0 / 0 | 0 / 0 | 4 / 0 | 0 / 0 | 0 / 0 | 3 / 0 | 7 |
| continuum-flow | 1 / 2 | 1 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 2 / 1 | 7 |
| continuum-git | 1 / 0 | 1 / 0 | 2 / 2 | 0 / 0 | 0 / 0 | 1 / 0 | 7 |
| jtag-mcp | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 5 / 0 | 0 / 0 | 5 |
| airc-test-fixtures | 1 / 0 | 0 / 0 | 2 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 3 |
| continuum-text-analysis | 0 / 0 | 0 / 0 | 0 / 0 | 2 / 0 | 0 / 0 | 1 / 0 | 3 |
| continuum-orm-derive | 0 / 0 | 0 / 0 | 0 / 0 | 1 / 0 | 0 / 0 | 0 / 0 | 1 |
| continuum-proving-types | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 1 | 1 |
| continuum-tool-protocol | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 1 / 0 | 1 |
| **all core crates** | **398 / 291** | **218 / 34** | **2923 / 238** | **145 / 9** | **839 / 184** | **2192 / 1250** | **8721** |

The floor crates (plan phase 5 sweeps go floor-up): `continuum-base` 15 stringly errors and 1
string identity, production only; `continuum-host` 5 + 2 waits, 2 unknowns, 1 contract;
`continuum-pack` does not exist on canary yet.

