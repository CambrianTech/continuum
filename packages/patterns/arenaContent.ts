/**
 * The `arena` activity's neutral `Content` body — a room's benchmark ARENA face.
 *
 * Benchmarks are the show ([[academy-learning-is-the-show]]): the arena room
 * renders live scoring feeds from real eval runs — the RESULTS ledger
 * (benchmarks/RESULTS.jsonl rows) ranked into per-benchmark leaderboards, plus
 * the in-flight run strip when an eval is live. It is a room PURPOSE
 * (`ARENA_PURPOSE`), reached by the same nav semantics as any activity and
 * rendered by a purpose-registered `Content` renderer — the arena sibling of
 * `'chat'` / `'foundry'` / `'persona'` / `'live'`. Shapes only: consumer-neutral,
 * DOM-free, ANSI-free.
 *
 * Honesty contract ([[fallbacks-are-illegal-fail-loud]]):
 *   - every row is a REAL ledger row (score, total, pass rate, machine
 *     provenance, captured date, git sha) — provenance is a REQUIRED column
 *     ([[benchmark-numbers-carry-gpu-provenance]]), never dropped for looks;
 *   - `excluded` rows stay visible as excluded (struck, not hidden) — the
 *     ledger is an audit surface, not a highlight reel;
 *   - `feedLive` is true only when a live core envelope stream is attached;
 *     a static projection renders with the honest "ledger snapshot" banner;
 *   - `liveRun` exists only while an eval is actually running.
 */

/** The `Content` purpose key the arena face dispatches on. A room recipe that
 *  declares this purpose IS an arena room. */
export const ARENA_PURPOSE = 'arena';

/** One scored ledger row — a real eval result (RESULTS.jsonl row, projected). */
export interface ArenaResultRowVM {
  /** Benchmark id ("humaneval-rs", "hard-rs", …). */
  readonly benchmark: string;
  /** Model / persona under test. */
  readonly model: string;
  /** The arm ("RAW", "OURS", "SYSTEM", …) — which harness framing ran. */
  readonly arm: string;
  /** Solved count. */
  readonly score: number;
  /** Task count. */
  readonly total: number;
  /** score/total as 0..1 — the ranking key. */
  readonly passRate: number;
  /** Capture date label (ledger `captured`, e.g. "2026-07-08"). */
  readonly captured: string;
  /** Hardware provenance — REQUIRED display column, never dropped. */
  readonly machine: string;
  /** Ledger note (methodology aside); absent = none. */
  readonly note?: string;
  /** Excluded from headline claims — still RENDERED (struck), never hidden. */
  readonly excluded: boolean;
}

/** One ranked leaderboard — a benchmark's rows, best pass-rate first. */
export interface ArenaBoardVM {
  readonly benchmark: string;
  /** Ranked rows (rank = index + 1). Excluded rows sort after included ones. */
  readonly rows: readonly ArenaResultRowVM[];
}

/** The in-flight run strip — present ONLY while an eval is actually running. */
export interface ArenaLiveRunVM {
  readonly benchmark: string;
  readonly model: string;
  /** Tasks completed so far. */
  readonly done: number;
  /** Total tasks in the run. */
  readonly total: number;
  /** Current task label ("task 12: parse_fen"), absent between tasks. */
  readonly currentTask?: string;
}

/** The arena face body, fully projected. */
export interface ArenaContentBody {
  /** Per-benchmark leaderboards, already ranked. Empty = no ledger rows. */
  readonly boards: readonly ArenaBoardVM[];
  /** The live run strip; absent = no eval running (honest absence). */
  readonly liveRun?: ArenaLiveRunVM;
  /** True only when a live core feed is attached; false = ledger snapshot. */
  readonly feedLive: boolean;
  /** Ledger row count BEFORE grouping (the audit denominator). */
  readonly rowCount: number;
}
