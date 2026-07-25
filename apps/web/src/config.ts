/**
 * Runtime config resolution for the web chat client — fail-loud, no invented
 * defaults.
 *
 * Two things the browser can't know on its own and must NOT fabricate:
 *   - `wsUrl` — where the headless core's WS ingress listens. The core binds
 *     `127.0.0.1:$CONTINUUM_CORE_WS` only when that env is set (there is no
 *     hardcoded port), so guessing one would silently point at nothing. Resolve
 *     it explicitly or fail loud ([[fallbacks-are-illegal-fail-loud]]).
 *   - `senderId` — WHO the human is. The WS transport establishes no identity yet
 *     (`session()` is `{}`; the pairing handshake is identity-substrate work,
 *     tasks #37/#38), and `chat/send` needs a real sender UUID. Minting a random
 *     one would create a ghost citizen, so this is explicit config until identity
 *     lands, at which point `session().userId` becomes the source and this the
 *     override.
 *
 * Resolution order (first hit wins): URL query param → Vite build env. Query
 * params make a running build repointable without a rebuild (`?core=…&me=…`);
 * the env is the packaged default.
 */

/** Resolved client configuration — everything the app needs to reach one room. */
export interface WebChatConfig {
  /** WS URL of the core's thin-client ingress (e.g. `ws://127.0.0.1:8974`). */
  readonly wsUrl: string;
  /** ws URL of the core's live CALL server (media plane). Optional — absent
   *  keeps the live face avatar-presence with the mic honestly disabled.
   *  `?call=ws://host:port` / VITE_CONTINUUM_CALL_WS; defaults to the core
   *  host's :8790 (the boot default CONTINUUM_CALL_WS). */
  readonly callUrl?: string;
  /** The human citizen's UUID, threaded as `chat/send`'s `senderId`. */
  readonly senderId: string;
}

/** Read a value from `?key=` then `import.meta.env[VITE_key]`, else `undefined`. */
function lookup(queryKey: string, envKey: string): string | undefined {
  const params = new URLSearchParams(globalThis.location.search);
  const fromQuery = params.get(queryKey);
  if (fromQuery) return fromQuery;
  const env = (import.meta as { env?: Record<string, string | undefined> }).env;
  const fromEnv = env?.[envKey];
  return fromEnv && fromEnv.length > 0 ? fromEnv : undefined;
}

/**
 * Resolve the client config or throw naming exactly what's missing and how to
 * supply it. Called once at boot; a missing value is an operator setup error,
 * surfaced immediately rather than as a dead, blank UI.
 */
export function resolveConfig(): WebChatConfig {
  const wsUrl = lookup('core', 'VITE_CONTINUUM_WS');
  const callUrl =
    lookup('call', 'VITE_CONTINUUM_CALL_WS') ??
    (wsUrl !== undefined ? wsUrl.replace(/:\d+$/, ':8790') : undefined);
  const senderId = lookup('me', 'VITE_CONTINUUM_USER_ID');

  // Narrow on the values themselves (not a separate count) so TS proves both are
  // `string` past this block — no cast, no `!`, just a guard that actually holds.
  if (wsUrl === undefined || senderId === undefined) {
    const missing: string[] = [];
    if (wsUrl === undefined) missing.push("core WS url — set VITE_CONTINUUM_WS (or ?core=ws://host:port). The core must run with CONTINUUM_CORE_WS=<port> set.");
    if (senderId === undefined) missing.push('sender identity — set VITE_CONTINUUM_USER_ID (or ?me=<uuid>). Identity pairing is not wired yet (tasks #37/#38).');
    throw new Error(`web chat config incomplete:\n  - ${missing.join('\n  - ')}`);
  }

  return { wsUrl,
    callUrl, senderId };
}
