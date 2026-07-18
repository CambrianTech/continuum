/**
 * DomSurface integration spec — proves the three channels + diff end-to-end against a
 * self-contained `data:` fixture (no server, deterministic). This is the outlier-A proof
 * that the `Surface` contract is real: a persona can SEE (render), REASON (probe:
 * DOM + a11y), ACT (click + hot-swap CSS), and MEASURE THE CHANGE (diff).
 *
 * Launches a real headless Chromium-family browser (resolveLaunch → installed Chrome/
 * Chromium/Brave/Edge/Opera), so it's an integration test, not a unit test — hence the
 * generous timeout. It needs a Chromium browser present; that's our stated expectation.
 */

import { describe, it, expect, afterEach } from 'vitest';
import { DomSurface } from './domSurface';
import type { ProbeNode } from './surface';

/** A tiny page: a heading + a button that, on click, rewrites the heading AND repaints
 *  the background — so a click produces BOTH a structural change (probe) and a visual
 *  change (diff). */
const FIXTURE = `data:text/html,${encodeURIComponent(`
<!doctype html><html><head><style>
  body { margin:0; background:#111318; color:#e6e9ef; font:16px system-ui; }
  h1 { padding:24px; margin:0; font-size:28px; }
  button { margin:0 24px; padding:10px 18px; font-size:16px; }
</style></head><body>
  <h1 id="title">Before</h1>
  <button id="go" aria-label="Change it"
    onclick="document.getElementById('title').textContent='After'; document.body.style.background='#1f6f4a';">
    Change it
  </button>
</body></html>`)}`;

/** Flatten a probe tree to every node, for membership assertions. */
function flatten(node: ProbeNode, out: ProbeNode[] = []): ProbeNode[] {
  out.push(node);
  for (const c of node.children) flatten(c, out);
  return out;
}
const hasText = (root: ProbeNode, t: string): boolean => flatten(root).some((n) => n.text === t);
const hasName = (root: ProbeNode | undefined, n: string): boolean =>
  root ? flatten(root).some((x) => x.name === n) : false;

describe('DomSurface — the web Surface (Percept · Probe · Actuator · diff)', () => {
  let surface: DomSurface | undefined;
  afterEach(async () => {
    await surface?.close();
    surface = undefined;
  });

  // what this catches: the whole Surface contract for the DOM in one flow — a regression
  // in any channel (blank render, empty probe, a no-op act, a diff that can't decode)
  // fails here. This is the outlier-A that the Surface trait is validated against.
  it('renders, probes structure + a11y, acts, and measures the change', { timeout: 45_000 }, async () => {
    surface = await DomSurface.open({ url: FIXTURE, viewport: { width: 640, height: 240 } });

    // SEE — a real PNG with real dimensions (deviceScaleFactor 2 → 1280×480).
    const before = await surface.render();
    expect(before.kind).toBe('image');
    if (before.kind !== 'image') throw new Error('unreachable');
    expect(before.bytes.length).toBeGreaterThan(0);
    expect(before.width).toBeGreaterThan(0);
    expect(before.height).toBeGreaterThan(0);

    // REASON — the DOM tree carries the heading text; the a11y tree carries the button's
    // accessible name. Both channels present.
    const s0 = await surface.probe();
    expect(s0.tree.tag).toBe('body');
    expect(hasText(s0.tree, 'Before')).toBe(true);
    expect(hasName(s0.a11y, 'Change it')).toBe(true);

    // ACT — click the button; the heading must actually change in the structure.
    await surface.act({ kind: 'click', selector: '#go' });
    const s1 = await surface.probe();
    expect(hasText(s1.tree, 'After')).toBe(true);
    expect(hasText(s1.tree, 'Before')).toBe(false);

    // MEASURE — the visual delta of the click is non-trivial (bg repaint + text).
    const after = await surface.render();
    const d = surface.diff(before, after);
    expect(d.totalPixels).toBe(before.width * before.height);
    expect(d.ratio).toBeGreaterThan(0);

    // HOT-SWAP — inject CSS with NO reload, and prove it moved pixels (the fast-iteration seam).
    await surface.act({ kind: 'injectCss', css: 'body{background:#c026d3 !important;}' });
    const swapped = await surface.render();
    expect(surface.diff(after, swapped).ratio).toBeGreaterThan(0);
  });

  // what this catches: an identical before/after must diff to ~zero — the money signal is
  // trustworthy (no false "it changed" when nothing did).
  it('diffs identical renders to zero', { timeout: 45_000 }, async () => {
    surface = await DomSurface.open({ url: FIXTURE, viewport: { width: 320, height: 160 } });
    const a = await surface.render();
    const b = await surface.render();
    expect(surface.diff(a, b).ratio).toBe(0);
  });
});
