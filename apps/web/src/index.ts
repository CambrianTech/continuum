/**
 * Web chat client entry — wires the SDK to the `<chat-widget>` view.
 *
 * This is the ONLY file that touches both the SDK and the DOM host; it is the
 * composition root Joel's three-panel design hangs off of. Two sockets to the
 * same core WS ingress, each doing one thing:
 *   - READ  — a `StateConnection` subscribed to `kind="chat"`. Every envelope is
 *     merged into a `ChatState` and pushed onto `widget.state`; Lit re-renders
 *     the who/what/where panels. This is the positron read surface (#84) the
 *     persona also observes — same substrate, different client.
 *   - SEND  — a `Continuum` command client. "Talk to Asha normally" = one
 *     `chat/send` into the room the widget is currently showing. Asha's reply
 *     (and the echo of our own turn) arrives back through the READ stream, so
 *     there is no optimistic local append to drift out of sync.
 *
 * The widget itself imports neither socket — the app owns the wiring, the widget
 * owns the view ([[headless-core-many-clients]], [[persona-is-a-client]]).
 */

import {
  Continuum,
  WebSocketTransport,
  StateConnection,
  type StateEnvelope,
} from '@continuum/sdk-typescript';
import { resolveConfig } from './config';
import { ChatWidget, type SendHandler } from './chat/ChatWidget';
import { CHAT_KIND, chatStateFromEnvelope, type ChatState } from './chat/ChatState';

// Importing the module registers `<chat-widget>` as a side effect; keep the
// symbol referenced so bundlers don't tree-shake the definition away.
void ChatWidget;

async function main(): Promise<void> {
  const config = resolveConfig();

  const widget = document.createElement('chat-widget');
  const mount = document.getElementById('app') ?? document.body;
  mount.replaceChildren(widget);

  // The latest snapshot the READ stream has delivered — the SEND path reads its
  // `room_id` so a message always targets the room on screen.
  let latest: ChatState | undefined;

  // SEND socket: the command client. Fails loud if the send lands before any
  // snapshot named a room (no room to send into is a real error, not a no-op).
  const continuum = Continuum.connect(new WebSocketTransport(config.wsUrl));
  const sendHandler: SendHandler = async (text: string) => {
    if (!latest) {
      throw new Error('cannot send before the first room snapshot arrived — the room is unknown.');
    }
    const result = await continuum.commands.execute('chat/send', {
      roomId: latest.room_id,
      senderId: config.senderId,
      text,
    });
    // A kernel-level failure already rejected in the transport (this line never
    // runs). Belt-and-suspenders for any handler that instead reports failure
    // in-band: an explicit `success === false` must throw so the widget shows it
    // and keeps the draft — never a silently-dropped message.
    if (result.success === false) {
      throw new Error(`chat/send rejected: ${result.error ?? 'unknown error'}`);
    }
    // A `warning` on a success means stored-locally-but-broadcast-failed. Surface
    // it loud; the message did persist, so this is not a failure to throw on.
    if (result.warning) {
      console.warn(`chat/send partial: ${result.warning}`);
    }
  };
  widget.sendHandler = sendHandler;

  // READ socket: subscribe to chat state, merge each envelope into the widget.
  const state = new StateConnection(config.wsUrl);
  state.on(CHAT_KIND, (envelope: StateEnvelope) => {
    latest = chatStateFromEnvelope(envelope);
    widget.state = latest;
  });
  state.onClose((reason) => {
    // Never silently freeze: a dropped state feed is a stale-UI signal.
    console.error(`chat state feed closed: ${reason}`);
  });
  await state.connect();
}

main().catch((err) => {
  // Boot failure (bad config, dead core) must be visible, not a blank page.
  console.error('web chat client failed to start:', err);
  const mount = document.getElementById('app') ?? document.body;
  const pre = document.createElement('pre');
  pre.style.cssText = 'padding:24px;color:#f77;font:13px/1.5 ui-monospace,monospace;white-space:pre-wrap';
  pre.textContent = `Continuum web chat failed to start:\n\n${err instanceof Error ? err.message : String(err)}`;
  mount.replaceChildren(pre);
});
