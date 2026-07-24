/**
 * Vite config for @continuum/web.
 *
 * Two responsibilities, both about REAL data reaching the shell chrome:
 *   - `__APP_VERSION__` — the version badge's source of truth is this package's
 *     own manifest, stamped at build/dev time. The renderer never hardcodes a
 *     version literal; it draws whatever the build actually is.
 *   - `/avatars/*` (dev + preview servers) — serves the node's avatar store
 *     (`~/.continuum/avatars/<peer-id>.png`) so persona tiles can draw REAL
 *     avatar images without a separate asset server. The core carries the
 *     avatar *URL* on the roster slot (honest-absent when no file exists);
 *     this middleware is the thin static mapping for the browser dev loop.
 *     A packaged deployment serves the same path from its own static tier.
 */

import { defineConfig, type Plugin } from 'vite';
import { readFileSync, existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, normalize } from 'node:path';
import type { ServerResponse } from 'node:http';

const pkg = JSON.parse(
  readFileSync(new URL('./package.json', import.meta.url), 'utf8'),
) as { version: string };

/** Serve `~/.continuum/avatars/<file>.png` at `/avatars/<file>.png` (dev only). */
function avatarStore(): Plugin {
  const root = join(homedir(), '.continuum', 'avatars');
  const handler = (url: string | undefined, res: ServerResponse, next: () => void): void => {
    if (!url || !url.startsWith('/avatars/')) return next();
    // Strip query, decode, and confine to the store (no traversal).
    const name = decodeURIComponent(url.slice('/avatars/'.length).split('?')[0] ?? '');
    const file = normalize(join(root, name));
    if (!file.startsWith(root) || !file.endsWith('.png') || !existsSync(file)) {
      res.statusCode = 404;
      return res.end();
    }
    res.setHeader('content-type', 'image/png');
    res.setHeader('cache-control', 'no-cache');
    res.end(readFileSync(file));
  };
  return {
    name: 'continuum-avatar-store',
    configureServer(server) {
      server.middlewares.use((req, res, next) => handler(req.url, res, next));
    },
    configurePreviewServer(server) {
      server.middlewares.use((req, res, next) => handler(req.url, res, next));
    },
  };
}

export default defineConfig({
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  plugins: [avatarStore()],
});
