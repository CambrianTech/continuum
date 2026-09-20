/**
 * Terminal chat client entry — wires the SDK to the ANSI renderer.
 *
 * The exact twin of apps/web's `src/index.ts`: the ONLY file that touches both
 * the SDK and the output surface (here, stdout/stdin rather than the DOM). Same
 * two sockets to the same core WS ingress, each doing one thing:
 *   - READ  — a `StateConnection` subscribed to `kind="chat"`. Every envelope is
 *     merged into a `ChatState` and repaints the who/what/where frame. Same
 *     positron read surface (#84) the persona and the web widget observe.
 *   - SEND  — a `Continuum` command client. Each stdin line is one `chat/send`
 *     into the room on screen. Asha's reply (and the echo of our own turn) comes
 *     back through the READ stream, so there is no optimistic local append.
 *
 * The renderer imports neither socket — the app owns the wiring, the renderer
 * owns the view ([[headless-core-many-clients]], [[persona-is-a-client]]). That
 * this composition root and apps/web's differ only in surface (readline+ANSI vs
 * DOM+Lit), reusing the identical SDK seam and shared `@continuum/chat-view`
 * projection, is the outlier-B validation of task #29.
 */

import { createInterface } from 'node:readline';
import { stdin, stdout, argv, env, exit } from 'node:process';
import {
  Continuum,
  WebSocketTransport,
  StateConnection,
  type StateEnvelope,
} from '@continuum/sdk-typescript';
import { chatStateFromEnvelope, CHAT_KIND, chatViewModel, type ChatState } from '@continuum/chat-view';
import { resolveConfig } from './config.js';
import { renderChat } from './renderChat.js';

/** Clear the screen and home the cursor — the frame boundary the pure renderer
 *  deliberately does NOT emit (so it stays testable on plain text). */
const CLEAR_HOME = '\x1b[2J\x1b[H';

async function main(): Promise<void> {
  const config = resolveConfig(argv.slice(2), env);

  // The latest snapshot the READ stream delivered — the SEND path reads its
  // `room_id` so a message always targets the room on screen.
  let latest: ChatState | undefined;
  let lastError = '';

  /** Repaint the whole frame: header + roster + messages + a compose prompt. */
  function paint(): void {
    const body = latest
      ? renderChat(chatViewModel(latest))
      : '\x1b[2mConnecting to the room…\x1b[0m';
    const errorLine = lastError ? `\n\x1b[31mSend failed: ${lastError}\x1b[0m` : '';
    stdout.write(`${CLEAR_HOME}${body}${errorLine}\n\n> `);
  }

  // SEND socket: the command client. A kernel-level failure rejects in the
  // transport; an in-band `success === false` is thrown too — never a silently
  // dropped message ([[fallbacks-are-illegal-fail-loud]]).
  const continuum = Continuum.connect(new WebSocketTransport(config.wsUrl));
  async function send(text: string): Promise<void> {
    if (!latest) {
      throw new Error('cannot send before the first room snapshot arrived — the room is unknown.');
    }
    const result = await continuum.commands.execute('chat/send', {
      roomId: latest.room_id,
      senderId: config.senderId,
      text,
    });
    if (!result.success) {
      throw new Error(`chat/send rejected: ${result.error ?? 'unknown error'}`);
    }
    if (result.warning) {
      // Stored-locally-but-broadcast-failed: surface loud, but it did persist.
      lastError = `partial: ${result.warning}`;
    }
  }

  // READ socket: subscribe to chat state, repaint on each envelope.
  const state = new StateConnection(config.wsUrl);
  state.on(CHAT_KIND, (envelope: StateEnvelope) => {
    latest = chatStateFromEnvelope(envelope);
    paint();
  });
  state.onClose((reason) => {
    // Never silently freeze: a dropped feed is a stale-UI signal.
    stdout.write(`\n\x1b[31mchat state feed closed: ${reason}\x1b[0m\n`);
  });
  await state.connect();

  // Compose loop: each entered line is one turn. A blank line just repaints.
  const rl = createInterface({ input: stdin, output: stdout, terminal: false });
  rl.on('line', (line: string) => {
    const text = line.trim();
    if (text.length === 0) {
      paint();
      return;
    }
    lastError = '';
    send(text)
      .then(() => {
        paint();
      })
      .catch((err: unknown) => {
        lastError = err instanceof Error ? err.message : String(err);
        paint();
      });
  });
  rl.on('close', () => exit(0));

  paint();
}

main().catch((err: unknown) => {
  // Boot failure (bad config, dead core) must be visible, not a silent hang.
  stdout.write(
    `\x1b[31mContinuum tui chat failed to start:\x1b[0m\n\n` +
      `${err instanceof Error ? err.message : String(err)}\n`,
  );
  exit(1);
});
