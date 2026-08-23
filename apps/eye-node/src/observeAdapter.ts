/**
 * observeAdapter — fulfils `perception/observe` by driving `@continuum/perception`
 * and mapping its internal `Observation` onto the ts-rs wire `ObserveResult`.
 *
 * This is the boundary the eye-node registers with `Commands.provide` /
 * `transport.provide('perception/observe', …)`. A persona anywhere on the grid
 * calls `perception/observe` with a `target` URL; the core routes it here; we
 * open a real browser, SEE the pixels + READ the structure, and hand back the
 * bare `ObserveResult`. The `target` is uniform — continuum's own positron UI, a
 * dev server a persona built in a project, a benchmark harness, a room/recipe
 * route — all are just URLs to open.
 *
 * The wire types (`ObserveResult`, `ProbeNode`, …) are imported type-only from
 * the single Rust-generated source (`protocol/typescript/perception`); the
 * internal surface types come from `@continuum/perception`. Keeping the two
 * distinct is exactly why the capability could be built + validated headlessly
 * before the command existed — the adapter maps one onto the other HERE.
 */

import { PerceptionSession } from '@continuum/perception';
import type { Percept, ProbeNode as SurfaceProbeNode } from '@continuum/perception';

import type { ObserveParams } from '../../../protocol/typescript/perception/ObserveParams';
import type { ObserveResult } from '../../../protocol/typescript/perception/ObserveResult';
import type { ObservedImage } from '../../../protocol/typescript/perception/ObservedImage';
import type { ProbeNode as WireProbeNode } from '../../../protocol/typescript/perception/ProbeNode';

/**
 * Open `params.target`, observe it, and return the bare wire result. Never
 * throws: an adapter-side failure comes back as `{ success: false, error }` —
 * the honest bare contract the persona reasons over (a fabricated observation
 * would be worse than a named failure). Always closes the browser.
 */
export async function observe(params: ObserveParams): Promise<ObserveResult> {
  let session: Awaited<ReturnType<typeof PerceptionSession.openWeb>> | undefined;
  try {
    session = await PerceptionSession.openWeb({
      url: params.target,
      viewport: params.viewport
        ? { width: params.viewport.width, height: params.viewport.height }
        : undefined,
    });

    // Clip the render to a selector when asked (SEE just that region); the probe
    // still returns the surface structure to REASON over.
    const obs = await session.observe(params.selector ? { selector: params.selector } : undefined);

    return {
      success: true,
      url: obs.structure.url,
      title: obs.structure.title,
      image: perceptToImage(obs.percept),
      // The geometry/DOM tree is the one a persona aims actions at; the a11y tree
      // has no wire slot (single `structure`), so we surface the layout tree.
      structure: mapNode(obs.structure.tree),
    };
  } catch (err) {
    return {
      success: false,
      error: err instanceof Error ? err.message : String(err),
    };
  } finally {
    if (session) await session.close();
  }
}

/** Encode the rendered frame as a `data:` URL (the wire has no raw-bytes slot).
 *  A filmstrip percept has no single-image wire mapping — reject it loud. */
function perceptToImage(percept: Percept): ObservedImage {
  if (percept.kind !== 'image') {
    throw new Error(`perception/observe: a '${percept.kind}' percept has no single-image wire form`);
  }
  const base64 = Buffer.from(percept.bytes).toString('base64');
  return {
    dataUrl: `data:${percept.mime};base64,${base64}`,
    width: percept.width,
    height: percept.height,
  };
}

/** Map the internal surface `ProbeNode` onto the wire `ProbeNode`. Structurally
 *  near-identical; this makes the projection explicit (and mutable-array/copy
 *  clean) rather than leaning on structural assignability. */
function mapNode(node: SurfaceProbeNode): WireProbeNode {
  return {
    tag: node.tag,
    role: node.role,
    name: node.name,
    text: node.text,
    box: node.box
      ? { x: node.box.x, y: node.box.y, width: node.box.width, height: node.box.height }
      : undefined,
    attrs: node.attrs ? { ...node.attrs } : undefined,
    style: node.style ? { ...node.style } : undefined,
    children: node.children.map(mapNode),
  };
}
