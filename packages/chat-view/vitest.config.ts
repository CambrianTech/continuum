/**
 * Vitest config for @continuum/chat-view.
 *
 * Exists for ONE reason: to pin the timezone where every platform can read it.
 *
 * Time-of-day rendering is viewer-local by design, so the fixed HH:MM
 * assertions in `chatViewModel.spec.ts`, `crossConsumer.spec.ts` and
 * `historyProjections.spec.ts` need a pinned zone to be deterministic on any
 * runner. That pin used to live in the npm script as `TZ=UTC vitest run`.
 *
 * POSIX env-prefix syntax is not portable. On Windows, npm runs scripts through
 * `cmd.exe`, which has no such form — the shell reads `TZ` as a command and the
 * run dies with:
 *
 *     'TZ' is not recognized as an internal or external command
 *
 * So this whole suite, and `@continuum/web`'s alongside it, could not be run at
 * all by a Windows contributor. It passed on Linux CI and on macOS, which is
 * precisely why it survived: a test that only fails on the platform nobody
 * checks is indistinguishable from a passing one until someone checks. Found
 * 2026-08-08 by running the suite on Windows rather than trusting it.
 *
 * The runner config is node context on every platform, so the pin belongs here
 * — the same reasoning `apps/web/vite.config.ts` already gives for keeping it
 * out of the specs. No cross-env dependency: the fix removes a portability
 * assumption instead of packaging a tool to satisfy it.
 */
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    env: { TZ: 'UTC' },
  },
});
