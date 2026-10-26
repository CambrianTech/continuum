import { describe, expect, it } from 'vitest';
import { RowHeights, windowFor } from './transcriptWindow';

const ids = Array.from({ length: 1000 }, (_, i) => `r${i}`);
function measured(px: number): RowHeights {
  const h = new RowHeights();
  for (const id of ids) h.record(id, px);
  return h;
}

describe('windowFor', () => {
  // what this catches: the DOM growing with the room. A 1000-row transcript in a
  // 600 px viewport renders the viewport plus one viewport each side, and the
  // spacers keep the full scroll length.
  it('renders the viewport plus one viewport of overscan each side', () => {
    const w = windowFor(ids, measured(50), { scrollTop: 25_000, clientHeight: 600 }, false);
    expect(w.end - w.start).toBeLessThanOrEqual(3 * 12 + 2);
    expect(w.start).toBe(488);
    expect(w.abovePx + (w.end - w.start) * 50 + w.belowPx).toBe(50_000);
  });

  // what this catches: following the live edge must render the newest rows whatever
  // scrollTop says (it lags the content that just arrived).
  it('pins to the newest rows when following the live edge', () => {
    const w = windowFor(ids, measured(50), { scrollTop: 0, clientHeight: 600 }, true);
    expect(w.end).toBe(1000);
    expect(w.belowPx).toBe(0);
    expect(w.start).toBeGreaterThan(960);
  });

  // what this catches: windowing on a guess. With nothing measured yet every row
  // renders once so the next pass windows from real heights; an unmeasured row
  // counts at the mean of the measured ones.
  it('renders everything before the first measurement, then estimates by the mean', () => {
    const none = windowFor(ids, new RowHeights(), { scrollTop: 0, clientHeight: 600 }, true);
    expect(none).toEqual({ start: 0, end: 1000, abovePx: 0, belowPx: 0 });
    const h = new RowHeights();
    h.record('r0', 40);
    h.record('r1', 80);
    expect(h.of('r999')).toBe(60);
    h.retain(new Set(['r1']));
    expect(h.mean).toBe(80);
  });
});
