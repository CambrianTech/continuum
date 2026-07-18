import { describe, it, expect } from 'vitest';
import { experienceWorkspace } from './experienceWorkspace';
import type { Experience, RosterViewState } from '@continuum/sdk-typescript';

describe('experienceWorkspace', () => {
  it('projects the manifest + rich roster into a Workspace with standing + vitals', () => {
    // what this catches: the path-3 join — the manifest supplies purpose (content
    // dispatch) + structural standing, the "roster" payload supplies rich display
    // (names/kind glyphs/vitals meters), and they compose into ONE WorkspaceView that
    // any RenderTarget draws. If the join dropped standing or the vitals meters, the
    // portal's roster HUD would go blank.
    const manifest: Experience = {
      purpose: 'chat',
      regions: [],
      affordances: [],
      membership: [
        { peerId: 'joel', standing: 'owner' },
        { peerId: 'asha', standing: 'member' },
      ],
    };
    const roster: RosterViewState = {
      room_id: 'room-1',
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
          vitals: { INT: 80, NRG: 60 },
        },
      ],
    };

    const ws = experienceWorkspace({ manifest, roster, contentBody: { messages: [] } });

    // content dispatches by the manifest's purpose; nav is the rooms/tab primitive.
    expect(ws.content.purpose).toBe('chat');
    expect(ws.nav.cells[0]?.group).toBe('chat');

    // left rail = a global widget stack; the roster is one `kind:'listing'` widget whose
    // body is the rich roster ListingView (names, glyphs, standing badge, vitals meters).
    expect(ws.left[0]?.kind).toBe('listing');
    const rosterView = ws.left[0]?.body as { cells: Array<{ id: string; title: string; glyph?: string; badges?: string[]; meters?: Record<string, number> }> };
    expect(rosterView?.cells).toHaveLength(2);
    const joel = rosterView.cells.find((c) => c.id === 'joel')!;
    const asha = rosterView.cells.find((c) => c.id === 'asha')!;
    expect(joel.title).toBe('Joel');
    expect(joel.glyph).toBe('🧑');
    expect(joel.badges).toContain('owner'); // manifest standing overlaid
    expect(asha.badges).toContain('agent'); // kind from the roster payload
    expect(asha.meters).toEqual({ INT: 80, NRG: 60 }); // vitals HUD data survives, lossless
  });

  it('handles no roster yet — left empty, nav still driven by the manifest', () => {
    // what this catches: before the first presence snapshot the manifest alone must
    // still yield a valid Workspace (empty roster), never a crash.
    const manifest: Experience = {
      purpose: 'benchmark/hard-rs',
      regions: [],
      affordances: [],
      membership: [],
    };
    const ws = experienceWorkspace({ manifest });
    expect(ws.left).toHaveLength(0);
    expect(ws.content.purpose).toBe('benchmark/hard-rs');
  });
});
