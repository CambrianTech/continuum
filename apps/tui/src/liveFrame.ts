/**
 * One-frame live render — connects to the running core's WS state ingress and
 * draws the FIRST live `chat` snapshot through the SAME surface the interactive
 * TUI runs (`renderChat ∘ chatViewModel ∘ chatStateFromEnvelope`), then exits.
 * Proves a real node client renders the live room off the state seam. Run with
 * `tsx` (resolves the SDK's TS source). Delete after the proof, or keep as a
 * headless smoke of the read path.
 */
import { StateConnection, type StateEnvelope } from '@continuum/sdk-typescript';
import { chatStateFromEnvelope, CHAT_KIND, chatViewModel } from '@continuum/chat-view';
import { renderChat } from './renderChat.js';

async function main(): Promise<void> {
  const wsUrl = process.env.CONTINUUM_WS ?? 'ws://127.0.0.1:8974';
  const state = new StateConnection(wsUrl);
  let rendered = false;
  state.on(CHAT_KIND, (env: StateEnvelope) => {
    if (rendered) return;
    rendered = true;
    process.stdout.write(renderChat(chatViewModel(chatStateFromEnvelope(env))) + '\n');
    process.exit(0);
  });
  await state.connect();
  setTimeout(() => {
    process.stderr.write('no chat state within 15s — room empty or WS unfed\n');
    process.exit(1);
  }, 15000);
}

main().catch((err: unknown) => {
  process.stderr.write(`live-frame failed: ${err instanceof Error ? err.message : String(err)}\n`);
  process.exit(1);
});
