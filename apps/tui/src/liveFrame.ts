/**
 * One-frame live render — the first real composition on positron's `mount()`.
 *
 * Connects to the running core's WS state ingress and draws the FIRST live `chat`
 * snapshot through the framework path: `mount(chatApp, source, createAnsiTarget(), sink)`.
 * `chatApp` projects the snapshot → the neutral WorkspaceView; the ANSI target paints
 * it; the sink writes the frame. The SAME `chatApp` the browser mounts on `webTarget`,
 * here mounted on the terminal target — define once, one line per surface. Run with
 * `tsx`. Proves a real node client renders the live room off the state seam via mount().
 */
import { StateConnection, type StateEnvelope } from '@continuum/sdk-typescript';
import { chatStateFromEnvelope, CHAT_KIND, chatApp, type ChatState } from '@continuum/chat-view';
import { mount, type AppSource } from '@continuum/patterns';
import { createAnsiTarget } from './ansiTarget.js';

async function main(): Promise<void> {
  const wsUrl = process.env.CONTINUUM_WS ?? 'ws://127.0.0.1:8974';
  const conn = new StateConnection(wsUrl);

  // The data source: push the FIRST chat snapshot, then done. Injected into mount() so
  // the same app runs against a real core, a replay, or a test fixture.
  const source: AppSource<ChatState> = (onState) => {
    let sent = false;
    conn.on(CHAT_KIND, (env: StateEnvelope) => {
      if (sent) return;
      sent = true;
      onState(chatStateFromEnvelope(env));
    });
    return () => {
      /* one-shot: process exits on first frame */
    };
  };

  // Define once, mount on the terminal target; the sink writes the frame and exits.
  mount(chatApp, source, createAnsiTarget(), (frame) => {
    process.stdout.write(frame + '\n');
    process.exit(0);
  });

  await conn.connect();
  setTimeout(() => {
    process.stderr.write('no chat state within 15s — room empty or WS unfed\n');
    process.exit(1);
  }, 15000);
}

main().catch((err: unknown) => {
  process.stderr.write(`live-frame failed: ${err instanceof Error ? err.message : String(err)}\n`);
  process.exit(1);
});
