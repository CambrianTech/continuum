/**
 * Experience source + web render — the path-3 join, proven headless.
 *
 * Drives the composite `ExperienceState` assembly (manifest + roster + content) through
 * `makeExperienceSource`, then renders the resulting `WorkspaceView` roster through the
 * REAL `webTarget`, and (DOM-free, like renderChat.spec) flattens the Lit template to
 * assert the members reach the markup. Proves: manifest structure + rich roster payload
 * → one Workspace → the web renderer draws the roster HUD — without a browser.
 */

import { describe, it, expect } from 'vitest';
import type { TemplateResult } from 'lit';
import type { StateEnvelope } from '@continuum/sdk-typescript';
import { experienceWorkspace, type ExperienceState } from '@continuum/experience-view';
import { webTarget } from '../render/litTarget';
import { makeExperienceSource, type OnKind } from './experienceSource';

/** Flatten a Lit `TemplateResult` (static strings + interpolated values, recursively)
 *  to text — the DOM-free assertion pattern renderChat.spec uses. */
function flatten(node: unknown): string {
  if (node == null || node === false) return '';
  if (Array.isArray(node)) return node.map(flatten).join('');
  if (typeof node === 'object' && 'strings' in node && 'values' in node) {
    const t = node as TemplateResult;
    let out = '';
    t.strings.forEach((s, i) => {
      out += s;
      if (i < t.values.length) out += flatten(t.values[i]);
    });
    return out;
  }
  return typeof node === 'string' || typeof node === 'number' ? String(node) : '';
}

describe('makeExperienceSource + webTarget', () => {
  it('assembles the composite state (manifest-first) and renders the roster HUD', () => {
    // A fake StateConnection.on: capture each kind's sink so we can fire envelopes.
    const sinks = new Map<string, (e: StateEnvelope) => void>();
    const on: OnKind = (kind, sink) => {
      sinks.set(kind, sink);
      return {
        off() {
          sinks.delete(kind);
        },
      };
    };

    const source = makeExperienceSource(on);
    let latest: ExperienceState | undefined;
    const teardown = source((s) => {
      latest = s;
    });

    // Roster arrives first — but NO emit until the manifest (structure-first).
    sinks.get('roster')!({
      kind: 'roster',
      payload: {
        room_id: 'r1',
        roster: [
          {
            member_id: 'joel',
            display_name: 'Joel',
            kind: { kind: 'human' },
            integrations: {},
            provenance: { runtime: 'interactive' },
            active: true,
            last_seen_ms: 0,
            vitals: {},
          },
          {
            member_id: 'asha',
            display_name: 'Asha',
            kind: { kind: 'agent' },
            integrations: {},
            provenance: { runtime: 'persona' },
            active: true,
            last_seen_ms: 0,
            vitals: { INT: 80 },
          },
        ],
      },
    } as StateEnvelope);
    expect(latest).toBeUndefined();

    // Manifest arrives → the composite is emitted.
    sinks.get('experience')!({
      kind: 'experience',
      payload: {
        purpose: 'chat',
        regions: [],
        affordances: [],
        membership: [
          { peerId: 'joel', standing: 'owner' },
          { peerId: 'asha', standing: 'member' },
        ],
      },
    } as StateEnvelope);

    expect(latest).toBeDefined();
    expect(latest!.manifest.purpose).toBe('chat');
    expect(latest!.roster?.roster).toHaveLength(2);

    // Render the roster rail-widget through the REAL webTarget → both members reach the
    // markup. `left[0]` is now a `kind:'listing'` PanelWidget (the roster), drawn via
    // `widget()` — the rail's dispatch seam.
    const ws = experienceWorkspace(latest!);
    expect(ws.left[0]!.kind).toBe('listing');
    const rosterMarkup = flatten(webTarget.widget(ws.left[0]!));
    expect(rosterMarkup).toContain('Joel');
    expect(rosterMarkup).toContain('Asha');

    // Teardown off()s every subscription.
    teardown();
    expect(sinks.size).toBe(0);
  });
});
