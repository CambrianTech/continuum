import { describe, it, expect } from 'vitest';
import { resolveConfig } from './config';

const UUID = '11111111-2222-3333-4444-555555555555';
const WS = 'ws://127.0.0.1:8974';

describe('resolveConfig (tui)', () => {
  it('resolves from CLI flags with a following value', () => {
    // what this catches: the `--flag value` argv shape not being parsed — the
    // primary way a launch is repointed inline.
    const cfg = resolveConfig(['--core', WS, '--me', UUID], {});
    expect(cfg).toEqual({ wsUrl: WS, senderId: UUID });
  });

  it('resolves from `--flag=value` form too', () => {
    // what this catches: the `=` form silently not splitting, leaving the value
    // glued to the key.
    const cfg = resolveConfig([`--core=${WS}`, `--me=${UUID}`], {});
    expect(cfg).toEqual({ wsUrl: WS, senderId: UUID });
  });

  it('falls back to environment variables when a flag is absent', () => {
    // what this catches: the env being ignored — it is the shell-profile default
    // and must fill in whatever flags omit.
    const cfg = resolveConfig([], { CONTINUUM_WS: WS, CONTINUUM_USER_ID: UUID });
    expect(cfg).toEqual({ wsUrl: WS, senderId: UUID });
  });

  it('prefers a flag over the environment for the same key', () => {
    // what this catches: precedence inversion — an inline `--core` must win over
    // a stale env value, not the reverse.
    const cfg = resolveConfig(['--core', WS], {
      CONTINUUM_WS: 'ws://wrong:1',
      CONTINUUM_USER_ID: UUID,
    });
    expect(cfg.wsUrl).toBe(WS);
  });

  it('fails loud naming BOTH missing values, never inventing a default', () => {
    // what this catches: a silent/blank start when config is absent — a guessed
    // ws url points at nothing and a minted senderId is a ghost citizen, so this
    // must throw and name exactly what to supply ([[fallbacks-are-illegal-fail-loud]]).
    expect(() => resolveConfig([], {})).toThrow(/core WS url/);
    expect(() => resolveConfig([], {})).toThrow(/sender identity/);
  });

  it('treats an empty-string flag/env as absent, not as a valid value', () => {
    // what this catches: `--core ''` or `CONTINUUM_WS=` sailing through as a
    // usable-but-empty url instead of tripping the fail-loud guard.
    expect(() => resolveConfig([], { CONTINUUM_WS: '', CONTINUUM_USER_ID: '' })).toThrow(
      /config incomplete/,
    );
  });
});
