/**
 * `messageDigest` — the transcript's digest tier (PERCEPTION-RESOLUTION-CONTRACT).
 *
 * No layer may flood: a message body past a threshold projects a DIGEST — the
 * head of the message plus a mechanical tail summary ("… +214 lines (38,102
 * chars)") and, when the collapsed remainder is dominated by one repeated
 * line/token, a repetition histogram line ("mostly 213× 'ae0e-'") — the
 * `sort | uniq -c` idea, in the view model. Full fidelity stays one affordance
 * away: the renderer shows the digest with an expand control; expanding reveals
 * the untouched original ([[perception-resolution-contract-no-layer-may-flood]]).
 *
 * Everything here is MECHANICAL string work — counting, slicing, dedup — never
 * an LLM summarizing, never a heuristic deciding "importance". Perception-side
 * compression, reader-side choice. The live incident this guards against: a
 * persona posted a degenerate repetition wall (hundreds of repeated "ae0e-"
 * tokens) into #general and the web chat rendered it full-height — thousands of
 * pixels of garbage flooding every human in the room.
 */

/** Digest when a body exceeds EITHER bound. ~16 lines is one comfortable
 *  screenful of transcript at the widget's 14px/1.45 rhythm (~450px — more and a
 *  single message monopolizes the viewport); ~1200 chars is that same screenful
 *  for unbroken prose at the bubble's 68ch measure (≈17 wrapped lines). Chosen
 *  for reading comfort, not model context — this is the HUMAN flood bound. */
export const DIGEST_OVER_CHARS = 1200;
export const DIGEST_OVER_LINES = 16;

/** The digest head — enough to identify the message (who's saying what kind of
 *  thing) without re-flooding: ~6 lines / ~300 chars, whichever caps first. */
export const DIGEST_HEAD_LINES = 6;
export const DIGEST_HEAD_CHARS = 300;

/** A repeated line/token owning MORE than this share of the collapsed remainder
 *  is called out by the histogram — the ">50% = degenerate repetition" bound. */
const HISTOGRAM_DOMINANCE = 0.5;

/** How much of a repeated line/token the histogram quotes before eliding. */
const HISTOGRAM_SAMPLE_CHARS = 24;

/** The digest projection of one over-threshold message body. */
export interface MessageDigestVM {
  /** The visible head — the first ~6 lines / ~300 chars, verbatim. */
  readonly head: string;
  /** Mechanical summary of what collapsed: "… +214 lines (38,102 chars)". */
  readonly tailSummary: string;
  /** Present when one repeated line/token owns >50% of the collapsed remainder:
   *  "mostly 213× 'ae0e-'" — the singleton-in-a-sea-of-repeats made visible. */
  readonly histogram?: string;
}

/** Deterministic thousands grouping (38102 → "38,102") — no locale machinery,
 *  so the view model renders identically on every machine. */
function groupThousands(n: number): string {
  return String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}

/** Quote a repeated line/token for the histogram, eliding a long one. */
function sample(text: string): string {
  return text.length > HISTOGRAM_SAMPLE_CHARS ? `${text.slice(0, HISTOGRAM_SAMPLE_CHARS)}…` : text;
}

/** `sort | uniq -c | sort -rn | head -1` over a list of units: the dominant unit
 *  and its count, or undefined when no single unit exceeds the dominance bound. */
function dominant(units: readonly string[]): { unit: string; count: number } | undefined {
  if (units.length < 2) return undefined;
  const counts = new Map<string, number>();
  for (const u of units) counts.set(u, (counts.get(u) ?? 0) + 1);
  let top: { unit: string; count: number } | undefined;
  for (const [unit, count] of counts) {
    if (!top || count > top.count) top = { unit, count };
  }
  return top && top.count / units.length > HISTOGRAM_DOMINANCE ? top : undefined;
}

/** The repetition histogram over the collapsed remainder — line-level first (the
 *  `uniq -c` unit that caught 111 identical DeprecationWarnings), then
 *  whitespace-token-level (the "ae0e- ae0e- ae0e-" wall shape). Undefined when
 *  nothing dominates — an honest absence, never a fabricated pattern. */
function repetitionHistogram(remainder: string): string | undefined {
  const lines = remainder
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l.length > 0);
  const byLine = dominant(lines);
  if (byLine) return `mostly ${byLine.count}× '${sample(byLine.unit)}'`;
  const tokens = remainder.split(/\s+/).filter((t) => t.length > 0);
  // A line-dominated wall already matched above; require enough tokens that
  // "half the tokens repeat" means a wall, not a short sentence echoing a word.
  const byToken = tokens.length >= 8 ? dominant(tokens) : undefined;
  return byToken ? `mostly ${byToken.count}× '${sample(byToken.unit)}'` : undefined;
}

/**
 * Classify a message body's display tier. Under both bounds → `undefined` (the
 * full tier: render verbatim). Over either → the digest projection. Pure and
 * deterministic; the renderer owns only the expand/collapse state.
 */
export function messageDigest(content: string): MessageDigestVM | undefined {
  const lines = content.split('\n');
  if (content.length <= DIGEST_OVER_CHARS && lines.length <= DIGEST_OVER_LINES) return undefined;

  let head = lines.slice(0, DIGEST_HEAD_LINES).join('\n');
  if (head.length > DIGEST_HEAD_CHARS) head = head.slice(0, DIGEST_HEAD_CHARS);

  // The remainder is everything the digest hides (minus the joining newline, so
  // the counts describe exactly the collapsed text, not the seam).
  let remainder = content.slice(head.length);
  if (remainder.startsWith('\n')) remainder = remainder.slice(1);
  const tailLines = remainder.length === 0 ? 0 : remainder.split('\n').length;
  const tailSummary = `… +${groupThousands(tailLines)} ${tailLines === 1 ? 'line' : 'lines'} (${groupThousands(remainder.length)} chars)`;

  const histogram = repetitionHistogram(remainder);
  return { head, tailSummary, ...(histogram ? { histogram } : {}) };
}
