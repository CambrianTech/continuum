/**
 * The `bench` activity's neutral `Content` body — the Academy's live benchmark
 * board (#374/#329: a benchmark IS a live room; the run rows ARE the panel).
 *
 * One row per active or recently-graded run — operator-launched AND
 * citizen-claimed on the SAME board (2026-08-08: an autonomous claim-run
 * contended invisibly with two operator rounds for two lanes, and only probe
 * archaeology surfaced it; on this board that hour is one glance). Each row
 * carries PROGRESS, never just liveness ([[a-serving-health-status-signal-
 * that-reports-liveness-instead-of-progress]] applied to benchmarks):
 * generations completed, age of the last one, edit acts, patch bytes, and the
 * verdict when graded — with a pass-to-pass REGRESSION rendered as the alarm
 * it is, never folded into a count.
 *
 * Shapes only: consumer-neutral, DOM-free, ANSI-free. The web/tui/desktop
 * renderers (and later every universe skin — the SCADA face is universe A)
 * are pure functions of this body; the recorded-fixture specs regression-test
 * exactly that seam.
 */

/** The `Content` purpose key the bench board dispatches on. A room recipe
 *  declaring this purpose IS a benchmark room (academy/bench/<run>). */
export const BENCH_PURPOSE = 'bench';

/** A graded attempt's verdict — the teachable facts, not just the counts. */
export interface BenchVerdictVM {
  readonly resolved: boolean;
  readonly f2pPassed: number;
  readonly f2pTotal: number;
  readonly p2pPassed: number;
  readonly p2pTotal: number;
  /** True when the patch broke previously-passing tests — the board renders
   *  this as an ALARM (the hidden-collateral lesson: a regression buried in a
   *  count taught "not fixed yet" when the truth was "destroyed the tree"). */
  readonly regression: boolean;
  /** Failed test NAMES (capped upstream) — a verdict that can teach. */
  readonly failedTests: readonly string[];
}

/** How a run's row reads RIGHT NOW. `queued` and `working` are distinct on
 *  purpose — the 2026-08-08 contention hour was 50 minutes of ambiguity
 *  between exactly those two states. */
export type BenchRunState =
  | 'queued' // attempt started, no generation completed yet
  | 'working' // generations flowing
  | 'grading' // attempt ended, verdict pending
  | 'resolved' // terminal: a graded attempt passed
  | 'failed' // terminal: final attempt graded, not resolved
  | 'stalled'; // watchdog fired — infra fault, never a capability verdict

/** One benchmark run — one board row. */
export interface BenchRunVM {
  readonly runId: string;
  /** Instance under test ("sympy__sympy-24066"). */
  readonly instance: string;
  /** The citizen working it ("Atlas") — the board shows WHO, always. */
  readonly persona: string;
  /** True when a citizen claimed this off the work board herself (vs an
   *  operator launch) — self-directed study is marked, never hidden. */
  readonly selfClaimed: boolean;
  readonly attempt: number;
  readonly maxAttempts: number;
  readonly state: BenchRunState;
  /** Full-generation acts completed this attempt. */
  readonly generations: number;
  /** Seconds since the last artifact write; null before the first —
   *  renders as the honest "no generations yet", never a fabricated pulse. */
  readonly lastGenAgeS: number | null;
  /** Edit/write acts so far — the leading indicator a patch is forming.
   *  Absent when the feed doesn't carry the edit/discovery split yet
   *  (kind="bench" v1) — the row hides the count rather than showing a
   *  fabricated 0. */
  readonly editActs?: number;
  /** Workspace diff size at last grading; null before any grade. */
  readonly patchBytes: number | null;
  /** Last graded attempt's verdict, when one exists. */
  readonly verdict?: BenchVerdictVM;
  /** The round this run belongs to (raw UUID) — the board groups by it. */
  readonly roundId?: string;
  /** The run's solve ROOM (raw UUID) — the DOOR a click navigates to. */
  readonly roomId?: string;
  /** The solve room's airc NAME — standing in a room requires JOINING it
   *  (by name) before selecting; a door without a name stays closed. */
  readonly roomName?: string;
}

/** One IN-FLIGHT round — the lifecycle truth behind the scoreboard (#371).
 *  Comes from the core's round tracker (reboot-durable), never derived by
 *  counting run rows client-side: a count is a guess, this is the state. */
export interface BenchRoundVM {
  /** Round id, compacted for display (which IS its run room's id). */
  readonly roundId: string;
  /** RAW round UUID — the grouping key run rows join on, and the run
   *  room's id a renderer may navigate to. */
  readonly rawId?: string;
  /** Suite as catalogued ("swe-bench-lite", "ds-1000"). */
  readonly benchmark: string;
  /** `working` | `done` — on the wire means in flight. */
  readonly stage: string;
  readonly dispatched: number;
  readonly settled: number;
  readonly remaining: number;
  /** `citizen` (in the room, feeds the curriculum) | `detached_solve`. */
  readonly driver: string;
  /** Glanceable health, pronounced core-side: `unstarted` | `grinding` |
   *  `stalled` | `paused` | `done`. Empty on a pre-verdict wire — render
   *  nothing, never guess (2026-09-01: `working 0/8` was pixel-identical
   *  for three hours of thrash and a healthy grind). */
  readonly verdict: string;
  /** Seconds since the newest work artifact on an unsettled card;
   *  null = no card has produced one yet (an absence, never 0). */
  readonly idleSecs: number | null;
  /** Per-card status, INCLUDING cards that never started — those have no
   *  run row and previously rendered as nothing at all. */
  readonly cards: readonly BenchRoundCardVM[];
}

/** One card of a round — WHAT, WHO, and how it is going. Cards with a live
 *  run also appear as full run cards; this row is the roll-call that makes
 *  the unstarted ones visible. */
export interface BenchRoundCardVM {
  readonly cardId: string;
  /** Instance under test; empty until the solve activity is minted. */
  readonly instance: string;
  /** Solver name once a run names one, else the staged assignee's uuid
   *  (compacted for display); empty = never staged. */
  readonly assignee: string;
  /** The solve activity's airc name — the navigable door; empty until minted. */
  readonly solveRoomName: string;
  /** `unstarted` | run phase (`active`, `quiet`, `ungraded`, …) | terminal. */
  readonly state: string;
  readonly acts: number | null;
  readonly patchBytes: number | null;
  readonly lastActSecs: number | null;
  readonly resolved: boolean | null;
  /** BOARD truth: who holds the card now (display name; '' = nobody). */
  readonly owner: string;
  /** BOARD truth: the card's column ('' when the board was unreadable). */
  readonly boardState: string;
  readonly gradedAtMs: number | null;
}

/** The bench board's content body. */
export interface BenchContentBody {
  /** Rows, most recently active first. Empty = the awaiting frame (the frame
   *  is the promise). */
  readonly runs: readonly BenchRunVM[];
  /** In-flight rounds from the round tracker — the scoreboard region's data.
   *  Empty = no rounds (honest; a pre-rounds wire folds to empty). */
  readonly rounds: readonly BenchRoundVM[];
  /** Lane pressure while runs contend — serving vs demanding lane counts from
   *  the live plan; absent when no serving feed. */
  readonly lanePressure?: { readonly serving: number; readonly demanding: number };
  /** True only when a live probe stream is attached — a static projection
   *  renders the honest "snapshot" banner (same contract as serving/arena). */
  readonly feedLive: boolean;
}
