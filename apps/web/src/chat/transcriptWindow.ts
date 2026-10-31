/**
 * The transcript's rendered window: only rows near the viewport are in the DOM.
 *
 * Every row ever paged used to stay in the DOM, so a long room grew the page until
 * the browser stalled. The window renders the rows inside the viewport plus one
 * viewport of overscan on each side; spacers stand in for the rest at their
 * measured heights (a row not yet measured counts at the mean of the measured
 * ones), so the scrollbar keeps the transcript's full length.
 */

/** Rendered heights by row id, from the rows the window has drawn. */
export class RowHeights {
  private readonly _px = new Map<string, number>();
  private _sum = 0;

  record(id: string, px: number): void {
    const prior = this._px.get(id);
    if (prior === px) return;
    this._sum += px - (prior ?? 0);
    this._px.set(id, px);
  }

  /** The mean measured height; 0 before anything was measured. */
  get mean(): number {
    return this._px.size === 0 ? 0 : this._sum / this._px.size;
  }

  of(id: string): number {
    return this._px.get(id) ?? this.mean;
  }

  /** Forget rows no longer in the transcript, so the map stays bounded with it. */
  retain(ids: ReadonlySet<string>): void {
    for (const [id, px] of this._px) {
      if (!ids.has(id)) {
        this._px.delete(id);
        this._sum -= px;
      }
    }
  }
}

/** Which rows render, and the spacer heights that stand in for the rest. */
export interface TranscriptWindow {
  readonly start: number;
  readonly end: number;
  readonly abovePx: number;
  readonly belowPx: number;
}

/** The scroll container's geometry, read from the DOM. */
export interface Viewport {
  readonly scrollTop: number;
  readonly clientHeight: number;
}

/** The window over `ids` for this viewport. `pinned` (the reader follows the live
 *  edge) anchors it at the newest row. Before any row is measured the mean is 0, so
 *  the whole transcript renders once and the next pass windows it from real heights. */
export function windowFor(
  ids: readonly string[],
  heights: RowHeights,
  viewport: Viewport,
  pinned: boolean,
): TranscriptWindow {
  const n = ids.length;
  const px = ids.map((id) => heights.of(id));
  const total = px.reduce((a, b) => a + b, 0);
  if (heights.mean === 0) return { start: 0, end: n, abovePx: 0, belowPx: 0 };
  const overscan = viewport.clientHeight;
  const top = pinned ? Math.max(0, total - viewport.clientHeight) : viewport.scrollTop;
  const from = top - overscan;
  const to = top + viewport.clientHeight + overscan;

  let start = 0;
  let abovePx = 0;
  while (start < n && abovePx + (px[start] ?? 0) <= from) {
    abovePx += px[start] ?? 0;
    start += 1;
  }
  let end = start;
  let bottom = abovePx;
  while (end < n && bottom < to) {
    bottom += px[end] ?? 0;
    end += 1;
  }
  return { start, end, abovePx, belowPx: total - bottom };
}
