/**
 * `gaugeWindowLabel` — the System panel's time-window chip must be DERIVED from
 * the gauge's own data (samples × cadence), never a hardcoded "1h"
 * ([[no-hardcoded-context-numbers-derive-from-the-live-window]] spirit).
 */

import { describe, it, expect } from 'vitest';
import { gaugeWindowLabel } from './SysPanel';

describe('gaugeWindowLabel', () => {
  // what this catches: the label is data-derived and honestly absent — no
  // series / no cadence → no chip, never an invented span.
  it('derives the span from samples × cadence and stays absent without data', () => {
    expect(gaugeWindowLabel(undefined)).toBeUndefined();
    expect(gaugeWindowLabel({ series: [] })).toBeUndefined();
    expect(
      gaugeWindowLabel({
        series: [{ label: 'CPU', points: Array<number>(90).fill(1), current: '1%' }],
        // 90 samples × 2s = 3 minutes.
        sampleIntervalMs: 2000,
      }),
    ).toBe('3m');
    expect(
      gaugeWindowLabel({
        series: [{ label: 'CPU', points: Array<number>(20).fill(1), current: '1%' }],
        sampleIntervalMs: 1000,
      }),
    ).toBe('20s');
    // No cadence reported → no label, even with points.
    expect(
      gaugeWindowLabel({ series: [{ label: 'CPU', points: [1, 2], current: '2%' }] }),
    ).toBeUndefined();
  });
});
