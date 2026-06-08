# apps/web — browser UI shell (placeholder)

**Status:** the legacy Node implementation lives at `src/{browser,widgets,server,daemons}/`
pending rewrite. Tracked by task #215.

## Intent

Thin browser shell consuming `sdk/typescript`. UI-only — DOM rendering,
event subscription, command dispatch through the SDK. No business logic
in `apps/web/`; substrate decisions stay in `core/`.

When this lands:
- `apps/web/package.json` declares only UI deps (lit, web components,
  vite or esbuild for the bundle).
- `apps/web/src/index.ts` imports from `@continuum/sdk-typescript` and
  attaches widgets to a continuum substrate via an existing `Connection`.
- The legacy `src/{browser,widgets,server,daemons}/` tree gets pruned
  once `apps/web/` is feature-parity with what the old daemons did
  (chat widget, persona inspector, screenshot tool, etc.).
