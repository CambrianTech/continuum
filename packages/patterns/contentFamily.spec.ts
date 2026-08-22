import { describe, it, expect } from 'vitest';
import { contentFamilyOf } from './contentFamily';

describe('contentFamilyOf', () => {
  // what this catches: the purpose→family map drifting from the shipped recipes.
  // Each assertion names the recipe that motivates it; a recipe whose purpose
  // stops resolving here renders `Interface error` in every client (#431).
  it('resolves every shipped recipe purpose to a registered family', () => {
    expect(contentFamilyOf('benchmark/hard-rs')).toBe('bench'); // recipes/benchmark.json
    expect(contentFamilyOf('benchmark/swe-lite')).toBe('bench'); // any benchmark variant
    expect(contentFamilyOf('video-chat')).toBe('live'); // recipes/video-chat.json
    expect(contentFamilyOf('profile')).toBe('persona'); // recipes/profile.json
    expect(contentFamilyOf('chat')).toBe('chat'); // recipes/chat.json (identity)
  });

  it('families pass through unchanged and unknown purposes are NOT rewritten', () => {
    for (const fam of ['chat', 'foundry', 'bench', 'live', 'persona', 'serving', 'grid', 'arena']) {
      expect(contentFamilyOf(fam)).toBe(fam);
    }
    // fail-loud preserved: the resolver widens what renders, never silences.
    expect(contentFamilyOf('mystery/activity')).toBe('mystery/activity');
  });
});
