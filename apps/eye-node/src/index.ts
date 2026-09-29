/**
 * Eye-node entry point. Resolves the core socket, starts the {@link EyeNode},
 * and stays alive fulfilling `perception/observe` until interrupted.
 *
 * Config (env):
 *   CONTINUUM_CORE_SOCKET  core IPC socket path or `tcp://host:port`. Required: the
 *                          launcher passes it from the core's endpoint resolver, and
 *                          the eye-node never guesses one (see coreEndpoint.ts).
 *   EYE_NODE_LABEL         provider label shown in core logs
 */

import { coreEndpoint } from './coreEndpoint';
import { EyeNode } from './eyeNode';

async function main(): Promise<void> {
  const resolved = coreEndpoint(process.env);
  if (!resolved.ok) {
    console.error(`eye-node: ${resolved.reason}`);
    process.exit(2);
  }
  const socketPath = resolved.endpoint;
  const label = process.env.EYE_NODE_LABEL;

  const eye = new EyeNode({ socketPath, label });

  let stopping = false;
  const shutdown = async (signal: string): Promise<void> => {
    if (stopping) return;
    stopping = true;
    console.log(`eye-node: ${signal} — disconnecting`);
    await eye.stop();
    process.exit(0);
  };
  const onSignal = (signal: string): void => {
    void shutdown(signal).catch((error) => {
      console.error('eye-node: shutdown failed', error);
      process.exit(1);
    });
  };
  process.on('SIGINT', () => onSignal('SIGINT'));
  process.on('SIGTERM', () => onSignal('SIGTERM'));

  console.log(`eye-node: connecting to core at ${socketPath} …`);
  // The start path spawns the eye-node BEFORE exec'ing the core, so the socket
  // may not exist yet — keep dialing until the core binds. After first bind,
  // the transport's serve-side self-healing owns reconnection across reboots.
  for (let attempt = 1; !stopping; attempt++) {
    try {
      await eye.start();
      break;
    } catch (err) {
      if (attempt === 1 || attempt % 15 === 0) {
        console.log(
          `eye-node: core not accepting yet (attempt ${attempt}) — retrying: ${err instanceof Error ? err.message : err}`,
        );
      }
      await new Promise((r) => setTimeout(r, 2000));
    }
  }
  console.log(
    'eye-node: registered perception/observe + perception/hot-edit — personas can now SEE and hot-edit. Ctrl-C to stop.',
  );

  // Stay alive; the transport's socket keeps the event loop busy, but guard
  // against any runtime that would otherwise exit an idle loop.
  await new Promise<never>(() => {});
}

main().catch((err) => {
  console.error('eye-node: fatal —', err instanceof Error ? err.message : err);
  process.exit(1);
});
