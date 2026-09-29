/**
 * Where the eye-node dials the core. The core's endpoint resolver is the ONE authority
 * for its socket (it differs by platform: Windows uses the system temp dir, not /tmp), and
 * the launcher passes it as `CONTINUUM_CORE_SOCKET`. The eye-node never guesses a path:
 * a hardcoded `/tmp/continuum-core.sock` default made it dial nothing on Windows while
 * the core listened elsewhere (card 4e8b1a92, 2026-09-28).
 */

/** The env var the launcher sets from the core's endpoint resolver. */
export const CORE_SOCKET_ENV = 'CONTINUUM_CORE_SOCKET';

export type CoreEndpoint = { ok: true; endpoint: string } | { ok: false; reason: string };

/** PURE: the core endpoint from the environment, or the reason there is none. */
export function coreEndpoint(env: Record<string, string | undefined>): CoreEndpoint {
  const endpoint = env[CORE_SOCKET_ENV]?.trim();
  if (endpoint) return { ok: true, endpoint };
  return {
    ok: false,
    reason:
      `${CORE_SOCKET_ENV} is not set. The launcher passes the core's endpoint (a socket path, ` +
      `or tcp://host:port); run the eye-node through continuum, or set ${CORE_SOCKET_ENV} to ` +
      `the path the core reports.`,
  };
}
