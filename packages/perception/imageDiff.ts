/**
 * `imageDiff` — the ONE pixel-diff every `Surface` uses (the compression rule: one place
 * computes "how much of the frame changed"). Both `DomSurface` and `SceneSurface` delegate
 * their `diff()` here, which is the whole point of the outlier-B exercise: JUDGE (`Delta`)
 * is universal — a rendered frame is a rendered frame regardless of who rendered it.
 *
 * The before/after ratio is the highest-value iteration signal and the cleanest training
 * label ("did my change do what I intended?") — see PERCEPTION-SURFACE.md §4.
 */

import pixelmatch from 'pixelmatch';
import { PNG } from 'pngjs';
import { ActError, type Delta, type Percept } from './surface';

/** Pixel delta between two `image/png` Percepts. Mismatched dimensions are treated as a
 *  full change (a layout-scale change), never a throw. Non-image Percepts fail loud. */
export function imageDiff(before: Percept, after: Percept): Delta {
  if (before.kind !== 'image' || after.kind !== 'image') {
    throw new ActError('imageDiff requires two image Percepts');
  }
  const a = PNG.sync.read(Buffer.from(before.bytes));
  const b = PNG.sync.read(Buffer.from(after.bytes));
  const total = a.width * a.height;
  if (a.width !== b.width || a.height !== b.height) {
    return { pixelsChanged: total, totalPixels: total, ratio: 1 };
  }
  // No diff-image output needed — just the mismatch count (pixelmatch 7 takes `void` here).
  const changed = pixelmatch(a.data, b.data, undefined, a.width, a.height, { threshold: 0.1 });
  return { pixelsChanged: changed, totalPixels: total, ratio: total === 0 ? 0 : changed / total };
}
