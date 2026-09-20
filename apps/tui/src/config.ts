/**
 * Runtime config resolution for the terminal chat client — fail-loud, no invented
 * defaults. The exact twin of apps/web's `resolveConfig` in intent, deliberately
 * different in mechanism: the browser reads `?query=` params, the terminal reads
 * `--flags` and the process environment. That the SAME two facts (`wsUrl`,
 * `senderId`) resolve here from a totally different source is part of the
 * outlier-B validation — the SDK client seam doesn't care where config came from.
 *
 * Two things the terminal can't know on its own and must NOT fabricate:
 *   - `wsUrl` — where the headless core's WS ingress listens. The core binds
 *     `127.0.0.1:$CONTINUUM_CORE_WS` only when that env is set (no hardcoded
 *     port), so a guessed URL points at nothing. Resolve it explicitly or fail
 *     loud ([[fallbacks-are-illegal-fail-loud]]).
 *   - `senderId` — WHO the human is. The WS transport establishes no identity yet
 *     (identity pairing is tasks #37/#38), and `chat/send` needs a real sender
 *     UUID. Minting one would create a ghost citizen, so it is explicit config
 *     until identity lands.
 *
 * Resolution order (first hit wins): CLI flag → environment variable. Flags make
 * a launch repointable inline (`--core ws://… --me <uuid>`); the env is the
 * shell-profile default.
 */

/** Resolved client configuration — everything the TUI needs to reach one room. */
export interface TuiChatConfig {
  /** WS URL of the core's thin-client ingress (e.g. `ws://127.0.0.1:8974`). */
  readonly wsUrl: string;
  /** The human citizen's UUID, threaded as `chat/send`'s `senderId`. */
  readonly senderId: string;
}

/**
 * Parse `--key value` and `--key=value` pairs out of an argv tail into a map.
 * Only long flags are recognized; bare positionals are ignored (the TUI takes
 * none). A flag with no following value (end of argv, or another `--flag` next)
 * maps to the empty string, which the resolver below treats as absent.
 */
function parseFlags(argv: readonly string[]): Map<string, string> {
  const flags = new Map<string, string>();
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (!arg?.startsWith('--')) continue;
    const body = arg.slice(2);
    const eq = body.indexOf('=');
    if (eq >= 0) {
      flags.set(body.slice(0, eq), body.slice(eq + 1));
      continue;
    }
    const next = argv[i + 1];
    if (next !== undefined && !next.startsWith('--')) {
      flags.set(body, next);
      i++;
    } else {
      flags.set(body, '');
    }
  }
  return flags;
}

/** Read a value from `--flag` then `env[envKey]`, else `undefined` (empty = absent). */
function lookup(
  flags: Map<string, string>,
  env: Record<string, string | undefined>,
  flagKey: string,
  envKey: string,
): string | undefined {
  const fromFlag = flags.get(flagKey);
  if (fromFlag && fromFlag.length > 0) return fromFlag;
  const fromEnv = env[envKey];
  return fromEnv && fromEnv.length > 0 ? fromEnv : undefined;
}

/**
 * Resolve the client config from argv + env, or throw naming exactly what's
 * missing and how to supply it. Pure in its inputs (argv/env passed in) so it is
 * unit-testable without touching `process`; `main` calls it with the real ones.
 */
export function resolveConfig(
  argv: readonly string[],
  env: Record<string, string | undefined>,
): TuiChatConfig {
  const flags = parseFlags(argv);
  const wsUrl = lookup(flags, env, 'core', 'CONTINUUM_WS');
  const senderId = lookup(flags, env, 'me', 'CONTINUUM_USER_ID');

  // Narrow on the values themselves (not a separate count) so TS proves both are
  // `string` past this block — no cast, no `!`, just a guard that actually holds.
  if (wsUrl === undefined || senderId === undefined) {
    const missing: string[] = [];
    if (wsUrl === undefined) {
      missing.push(
        'core WS url — pass --core ws://host:port or set CONTINUUM_WS. ' +
          'The core must run with CONTINUUM_CORE_WS=<port> set.',
      );
    }
    if (senderId === undefined) {
      missing.push(
        'sender identity — pass --me <uuid> or set CONTINUUM_USER_ID. ' +
          'Identity pairing is not wired yet (tasks #37/#38).',
      );
    }
    throw new Error(`tui chat config incomplete:\n  - ${missing.join('\n  - ')}`);
  }

  return { wsUrl, senderId };
}
