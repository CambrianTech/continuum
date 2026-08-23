/**
 * hotEditAdapter — fulfils `perception/hot-edit` ("hot css, no deployments") by
 * driving `@continuum/perception` and mapping the session's after-observation +
 * `Delta` onto the ts-rs wire `HotEditResult`.
 *
 * The sibling of {@link observe} (`observeAdapter.ts`), registered beside it by
 * the eye-node. A persona anywhere on the grid calls `perception/hot-edit` with
 * a `target` URL and a stylesheet; the core routes it here; we open the page,
 * observe a baseline, apply the CSS as the page's single hot-patch layer
 * (`<style data-continuum-hot-edit>`, REPLACED wholesale — the `hotPatchCss`
 * DomAction), re-observe, and hand back the bare wire result: the SAME
 * observation shape observe returns (the AFTER view of the page), plus the
 * applied-CSS echo and the before/after pixel `Delta` the session computes for
 * free.
 *
 * ## Scope honesty: stateless-but-effective (today)
 *
 * The page is re-opened per call — observe's session lifecycle. Each hot-edit
 * loads `target` FRESH and then applies `css`, so a persona iterating on a
 * single-file artifact re-applies its full accumulated stylesheet each call
 * (the replace-wholesale layer makes that idempotent). A persistent live
 * session (true in-place hot-editing, page state preserved across calls) is
 * the next step and changes only this adapter's session lifetime — never the
 * wire contract.
 *
 * ## Surface honesty
 *
 * `hotPatchCss` is a DOM verb. A scene surface has no style system; its `act`
 * rejects the foreign verb with `ActError` — which surfaces here as the honest
 * `{ success: false, error }`, never a silent no-op.
 */

import { PerceptionSession } from '@continuum/perception';

import type { HotEditParams } from '../../../protocol/typescript/perception/HotEditParams';
import type { HotEditResult } from '../../../protocol/typescript/perception/HotEditResult';

import { mapNode, perceptToImage } from './observeAdapter';

/**
 * Open `params.target`, apply `params.css` as the hot-patch layer, re-observe
 * (scoped to `params.selector` when given), and return the bare wire result.
 * Never throws: an adapter-side failure comes back as `{ success: false, error }`
 * with the applied-CSS echo intact. Always closes the browser.
 */
export async function hotEdit(params: HotEditParams): Promise<HotEditResult> {
  let session: Awaited<ReturnType<typeof PerceptionSession.openWeb>> | undefined;
  try {
    session = await PerceptionSession.openWeb({
      url: params.target,
      viewport: params.viewport
        ? { width: params.viewport.width, height: params.viewport.height }
        : undefined,
    });

    // Baseline from the SAME viewpoint the re-observation will use, so the
    // session's Delta frames exactly the pixels the patch moved (a selector
    // scopes both SEE passes; the probe still returns the whole structure).
    const view = params.selector ? { selector: params.selector } : undefined;
    await session.observe(view);

    const { observation, delta } = await session.interact(
      [{ kind: 'hotPatchCss', css: params.css }],
      view,
    );

    return {
      success: true,
      url: observation.structure.url,
      title: observation.structure.title,
      image: perceptToImage(observation.percept),
      // Same wire slot decision as observe: the geometry/DOM tree (the one a
      // persona aims at) fills the single `structure` slot.
      structure: mapNode(observation.structure.tree),
      appliedCss: params.css,
      delta: {
        pixelsChanged: delta.pixelsChanged,
        totalPixels: delta.totalPixels,
        ratio: delta.ratio,
      },
    };
  } catch (err) {
    return {
      success: false,
      error: err instanceof Error ? err.message : String(err),
      appliedCss: params.css,
    };
  } finally {
    if (session) await session.close();
  }
}
