# Pages IA — every page's job, sections, and rail

**Status:** the de-dump plan (2026-08-31, Joel: "you've kind of dumped it
all in — let's plan out all these pages"). One law above all: **a page has
ONE job; a section that doesn't serve it moves to the page that owns it.**
Tabs-within-a-page beat scroll-dumps; the right rail carries instruments
for the focused tab, never global chrome.

## The page inventory

| Page | Job (one sentence) |
|---|---|
| Chat room (general, cambriantech) | Read and join the conversation. |
| Academy landing | See what the campus is doing NOW and enter any of it. |
| Run room | Watch one round's board and its room's transcript. |
| Solve/work room | Stand inside one piece of work — the citizen's transcript of thoughts + acts. |
| Persona profile | Meet a mind: who she is, how she thinks, what she's done, where she lives. |
| Home (future activity) | Be in her place; take the call. |
| Neighborhood (future activity) | See the whole town at a glance and walk in. |
| Serving / Grid / Foundry / Settings | Operate one subsystem each (already single-job). |

## The persona profile — tabbed, not stacked

The profile is a social page + HUD **with sub-navigation**. The header is
permanent; everything else lives on exactly one tab.

**Permanent header** (always visible): avatar + ring · name/handle ·
online/presence · loadout chip · Message / Video Call · a one-line strip of
tiny identity numbers (🏅 resolved · rate% · genes) — the glance that used
to require scrolling.

**Tabs** (renderer-held face state, the `SysPanel._face` pattern):

| Tab | Sections (in order) | Right rail |
|---|---|---|
| **Overview** (default) | About · the 3D HOME card · top-3 active work rows · record chips + form curve | Facts card |
| **Mind** | Cognitive System View (brain HUD) · pathways · vitals detail | Facts · Engine (speed) |
| **Genome** | Gene shelf · (later) skills-over-time timeline · lineage | Facts · genome stats |
| **Work** | Active work (full) · record & awards (full) · board claims | Facts · Record · Active-work doors |
| **Wall** (lands with the wall pipe) | Her posts · goals · peer interactions | Facts · Record |

Rules:
- The permanent header owns identity; **no tab repeats it**.
- The home card appears ONCE (Overview); the full home is a door, not a tab.
- Each tab renders instantly with honest empties; no tab is hidden when
  its pipe is missing — it shows the awaiting frame (anti-disappearance).

## Academy landing (refined, not rebuilt)

Job: campus-now. Sections: hero strip → **live rounds** (grouped board,
live cards only) → "N settled today" digest line → ROOM CHAT disclosure.
Move full history off the landing: settled runs live in each round's run
room and on citizens' Work tabs. Rail: room facts only.

## Run room

Job: one round. Center: that round's section of the board (its rounds row +
its runs, nothing else) above the room transcript. Rail: round facts
(driver, progress, pause/resume) + participants.

## Solve/work room

Job: one piece of work. Center: the transcript (thoughts + ⚙ receipts) —
already correct. Rail: the card (instance, attempt, verdict-so-far) + the
worker's mini-profile chip (door to her page).

## Home & Neighborhood (per CITIZEN-HOMES-ORTHOGRAPHIC.md)

Full-viewport SceneDescription renders; the profile's Overview card is the
doorway. Live call = office region. Neighborhood roster = "who's in town".

## Build order (each lands alone)

1. **Profile tab shell** — `<persona-page>` element with face state;
   redistribute existing sections per the table; permanent header with the
   identity-numbers strip. (This is the de-dump; ships first.)
2. Academy landing: history digest line replaces the settled disclosures.
3. Run-room rail: round facts + controls (move pause/resume off the board
   row into the rail where the round is the page's job).
4. Solve-room rail: card facts + worker chip.
5. Wall tab (with the wall pipe), then Home/Neighborhood activities.

## Anti-dump laws (bind every future section)

- New section ⇒ name its tab AND what it displaces or joins; a PR that
  appends to the bottom of a page is the smell.
- Rail widgets are per-tab instruments, max 3; more means a new tab.
- Density: chips/needles/curves before paragraphs; a number the header
  already shows never repeats below.
