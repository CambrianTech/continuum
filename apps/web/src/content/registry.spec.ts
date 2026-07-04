/**
 * Content registry dispatch spec — the second-outlier proof.
 *
 * The same `webContentRegistry` routes a chat room to its conversation and a
 * foundry room to its model catalogue, keyed only on `purpose`, with no shell
 * change. Foundry's models render through the SAME generic `listingCell` the
 * roster uses — the first component the outlier earned. DOM-free: we flatten the
 * returned Lit template and assert every fact reaches the markup.
 */

import { describe, it, expect } from 'vitest';
import type { ForgeContentBody } from '@continuum/foundry-view';
import type { ForgeModelView } from '@continuum/sdk-typescript';
import { webContentRegistry } from './registry';

function flatten(node: unknown, out: string[] = []): string[] {
  if (typeof node === 'string') out.push(node);
  else if (typeof node === 'number') out.push(String(node));
  else if (Array.isArray(node)) for (const c of node as readonly unknown[]) flatten(c, out);
  else if (typeof node === 'object' && node !== null && 'strings' in node && 'values' in node) {
    const t = node as { strings: readonly string[]; values: readonly unknown[] };
    for (const s of t.strings) out.push(s);
    for (const v of t.values) flatten(v, out);
  }
  return out;
}
const markup = (n: unknown): string => flatten(n).join('');

describe('webContentRegistry — purpose dispatch', () => {
  const model = (over: Partial<ForgeModelView> = {}): ForgeModelView => ({
    model_id: 'qwen3-coder',
    display_name: 'Qwen3 Coder',
    source: 'huggingface',
    params_b: 30,
    ...over,
  });

  // what this catches: a foundry room's Content dispatches to the model catalogue
  // through the same generic listing-cell the roster uses — chat + foundry, one
  // registry, no shell change (the second-outlier proof). Each model is projected
  // by foundry-view's modelCell (title + "params · source" subtitle + source badge).
  it('renders a foundry room as its model catalogue', () => {
    const body: ForgeContentBody = {
      models: [model(), model({ model_id: 'x', display_name: 'Llama', params_b: undefined })],
    };
    const chunks = flatten(webContentRegistry.render({ purpose: 'foundry', body }));
    expect(chunks).toContain('Qwen3 Coder'); // projected title
    expect(chunks).toContain('30B · huggingface'); // subtitle: params + source
    expect(chunks).toContain('Llama'); // second model
    expect(chunks).toContain('huggingface'); // subtitle when params absent
  });

  // an empty catalogue is honest, not an error.
  it('renders an honest empty state for an empty catalogue', () => {
    const out = markup(webContentRegistry.render({ purpose: 'foundry', body: { models: [] } }));
    expect(out).toContain('No models');
  });

  // an unregistered purpose fails loud — an unknown activity is a wiring bug, never a blank.
  it('fails loud on an unregistered purpose', () => {
    expect(() => webContentRegistry.render({ purpose: 'no-such-activity', body: {} })).toThrow();
  });
});
