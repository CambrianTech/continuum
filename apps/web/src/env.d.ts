/**
 * Build-time constants injected by vite.config.ts `define`.
 *
 * `__APP_VERSION__` is the web client's package version — the version badge's
 * REAL source (never a hardcoded literal in a renderer). Vitest inherits the
 * same define through the shared vite config.
 */
declare const __APP_VERSION__: string;
