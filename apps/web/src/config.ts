/**
 * Runtime config resolution for the web chat client — zero-config first run,
 * explicit overrides.
 *
 * PREMISE CHANGE (2026-08-30, the fresh-user gauntlet): this file used to
 * fail loud on both values, and the very first thing the system's own eyes
 * (perception/observe) saw on a fresh desktop was its red error wall. Both
 * premises behind the fail-loud have since fallen:
 *   - `wsUrl` — the core now binds its WS ingress BY DEFAULT at
 *     `127.0.0.1:8974` (`DEFAULT_WS_PORT` in ipc/mod.rs; `CONTINUUM_CORE_WS=0`
 *     disables). Defaulting to the core's own boot default is resolving, not
 *     guessing — and a wrong host still fails visibly at connect, naming the
 *     URL it tried.
 *   - `senderId` — a per-load random UUID would be a ghost mill, but a
 *     PERSISTED one (minted once into localStorage) is precisely what a new
 *     human citizen at this browser IS. When identity pairing lands
 *     (tasks #37/#38), `session().userId` becomes the source and this the
 *     override.
 *
 * Resolution order (first hit wins): URL query param → Vite build env →
 * zero-config default. Query params make a running build repointable without
 * a rebuild (`?core=…&me=…`).
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

/** The core's boot-default WS ingress port (ipc/mod.rs `DEFAULT_WS_PORT`). */
const CORE_DEFAULT_WS_PORT = 8974;

/** localStorage key for this browser's minted-once human identity. */
const IDENTITY_KEY = 'continuum-web-identity';

/** This browser's persistent human identity: minted once, stable across loads.
 *  Storage-denied contexts (private windows with storage off) get a per-load
 *  identity — degraded but functional, never a blank screen. */
function persistentIdentity(): string {
  try {
    const existing = globalThis.localStorage.getItem(IDENTITY_KEY);
    if (existing && existing.length > 0) return existing;
    const minted = globalThis.crypto.randomUUID();
    globalThis.localStorage.setItem(IDENTITY_KEY, minted);
    return minted;
  } catch {
    return globalThis.crypto.randomUUID();
  }
}

export function resolveConfig(): WebChatConfig {
  const wsUrl =
    lookup('core', 'VITE_CONTINUUM_WS') ??
    `ws://${globalThis.location.hostname || '127.0.0.1'}:${CORE_DEFAULT_WS_PORT}`;
  const callUrl =
    lookup('call', 'VITE_CONTINUUM_CALL_WS') ?? wsUrl.replace(/:\d+$/, ':8790');
  const senderId = lookup('me', 'VITE_CONTINUUM_USER_ID') ?? persistentIdentity();

  return { wsUrl, callUrl, senderId };
}
