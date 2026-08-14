/**
 * Eye-node entry point. Resolves the core socket, starts the {@link EyeNode},
 * and stays alive fulfilling `perception/observe` until interrupted.
 *
 * Config (env):
 *   CONTINUUM_CORE_SOCKET  core IPC socket path or `tcp://host:port`
 *                          (default `/tmp/continuum-core.sock`, matching `uu`)
 *   EYE_NODE_LABEL         provider label shown in core logs
 */

import { EyeNode } from './eyeNode';

const DEFAULT_CORE_SOCKET = '/tmp/continuum-core.sock';

async function main(): Promise<void> {
  const socketPath = process.env.CONTINUUM_CORE_SOCKET ?? DEFAULT_CORE_SOCKET;
  const label = process.env.EYE_NODE_LABEL;

  const eye = new EyeNode({ socketPath, label });

  const shutdown = (signal: string) => {
    console.log(`eye-node: ${signal} — disconnecting`);
    eye.stop();
    process.exit(0);
  };
  process.on('SIGINT', () => shutdown('SIGINT'));
  process.on('SIGTERM', () => shutdown('SIGTERM'));

  console.log(`eye-node: connecting to core at ${socketPath} …`);
  await eye.start();
  console.log('eye-node: registered perception/observe — personas can now SEE. Ctrl-C to stop.');

  // Stay alive; the transport's socket keeps the event loop busy, but guard
  // against any runtime that would otherwise exit an idle loop.
  await new Promise<never>(() => {});
}

main().catch((err) => {
  console.error('eye-node: fatal —', err instanceof Error ? err.message : err);
  process.exit(1);
});
