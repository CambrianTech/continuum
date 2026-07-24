/**
 * renderLive (Lit) unit spec — the web surface of the live call face.
 *
 * DOM-free like renderChat.spec: flattens the returned Lit template tree
 * (static strings + interpolated values, recursively) and asserts every
 * projected fact reaches the markup — the reference grid's participants, the
 * speaking data-attribute, the live caption, the honest controls, and the
 * anti-disappearance empty state.
 */

import { describe, it, expect } from 'vitest';
import type { LiveContentBody } from '@continuum/patterns';
import { renderLive } from './renderLive';

interface LitTemplateLike {
  readonly strings: readonly string[];
  readonly values: readonly unknown[];
}
function isTemplateLike(node: object): node is LitTemplateLike {
  return 'strings' in node && 'values' in node;
}
function flatten(node: unknown, out: string[] = []): string[] {
  if (typeof node === 'string') out.push(node);
  else if (typeof node === 'number') out.push(String(node));
  else if (Array.isArray(node)) for (const child of node as readonly unknown[]) flatten(child, out);
  else if (typeof node === 'object' && node !== null && isTemplateLike(node)) {
    for (const s of node.strings) out.push(s);
    for (const v of node.values) flatten(v, out);
  }
  return out;
}

const body = (over: Partial<LiveContentBody> = {}): LiveContentBody => ({
  roomId: 'room-1',
  roomName: 'general',
  participants: [
    { id: 'asha', name: 'Asha', kind: 'agent', avatarUrl: '/avatars/asha.png', active: true, speaking: true, runtime: 'devstral' },
    { id: 'joel', name: 'Joel', kind: 'human', active: true, speaking: false, runtime: '' },
  ],
  caption: { speakerId: 'asha', speakerName: 'Asha', text: 'the planner admitted both lanes' },
  controls: {
    micAvailable: false,
    cameraAvailable: false,
    screenshareAvailable: false,
    captionsAvailable: true,
    captionsOn: true,
    hangupAvailable: true,
    transcriptCount: 22,
  },
  mediaPlaneLive: false,
  ...over,
});

describe('renderLive (Lit)', () => {
  // what this catches: every projected fact reaches the markup — participant
  // names, the speaker's caption text, the real transcript badge count, and
  // the honest "avatar presence" tag while the media plane isn't live.
  it('renders tiles, caption, and honest capability tag from the projected body', () => {
    const chunks = flatten(renderLive(body()));
    expect(chunks).toContain('Asha');
    expect(chunks).toContain('Joel');
    expect(chunks).toContain('/avatars/asha.png');
    expect(chunks).toContain('the planner admitted both lanes');
    expect(chunks).toContain('22'); // real transcript count badge
    const joined = chunks.join('');
    expect(joined).toContain('avatar presence'); // mediaPlaneLive: false → honest tag
    expect(joined).toContain('live-controls'); // controls bar always renders
  });

  // what this catches: the speaking border rides ONLY the projected speaking
  // flag — Asha's tile title carries "speaking", Joel's does not, and a
  // silent body renders no caption strip (never a fabricated line).
  it('speaking state and caption absence render honestly', () => {
    const speaking = flatten(renderLive(body())).join('');
    expect(speaking).toContain('Asha — speaking');
    expect(speaking).not.toContain('Joel — speaking');
    const { caption: _dropped, ...silentBody } = body();
    const silent = flatten(renderLive(silentBody)).join('');
    expect(silent).not.toContain('live-caption-name');
  });

  // what this catches: the anti-disappearance rule — an empty room renders its
  // honest empty state inside the same frame, never a blank center.
  it('an empty room renders the honest empty state', () => {
    const joined = flatten(renderLive(body({ participants: [] }))).join('');
    expect(joined).toContain('No one is in this room yet');
  });
});
