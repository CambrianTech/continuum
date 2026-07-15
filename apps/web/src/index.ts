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

import './theme.css';
import {
  Continuum,
  WebSocketTransport,
  StateConnection,
  type StateEnvelope,
} from '@continuum/sdk-typescript';
import { resolveConfig } from './config';
import { ChatWidget, type SendHandler } from './chat/ChatWidget';
import { CHAT_KIND, chatStateFromEnvelope, type ChatState } from '@continuum/chat-view';

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
    if (!result.success) {
      throw new Error(`chat/send rejected: ${result.error ?? 'unknown error'}`);
    }
    // A `warning` on a success means stored-locally-but-broadcast-failed. Surface
    // it loud; the message did persist, so this is not a failure to throw on.
    if (result.warning) {
      console.warn(`chat/send partial: ${result.warning}`);
    }
  };
  widget.sendHandler = sendHandler;

  // Visible connection diagnostics — a stuck "Connecting…" with no on-screen
  // reason is undebuggable. Surface the WS lifecycle so a blank/stuck tab tells
  // you WHY (socket closed / connected-but-no-snapshot / connect failed).
  const banner = document.createElement('div');
  banner.style.cssText =
    'position:fixed;top:0;left:0;right:0;z-index:9;padding:6px 12px;font:12px ui-monospace,monospace;background:#2a2a30;color:#cdcdd3;border-bottom:1px solid #3a3a42';
  const setStatus = (msg: string, warn = false): void => {
    banner.textContent = `positron: ${msg}`;
    banner.style.background = warn ? '#4a2a2a' : '#2a2a30';
    banner.style.color = warn ? '#f7b7b7' : '#cdcdd3';
    if (!banner.isConnected) document.body.appendChild(banner);
  };
  setStatus(`connecting to ${config.wsUrl} …`);

  // READ socket: subscribe to chat state, merge each envelope into the widget.
  let gotState = false;
  const state = new StateConnection(config.wsUrl);
  state.on(CHAT_KIND, (envelope: StateEnvelope) => {
    gotState = true;
    banner.remove();
    latest = chatStateFromEnvelope(envelope);
    widget.state = latest;
  });
  // #170 live typing: grow a transient bubble per persona as its turn streams in.
  // Ephemeral — the durable message still arrives via the CHAT_KIND sink above, which
  // supersedes the bubble. The widget filters to its current room + retires on `done`.
  state.onStreamDelta((delta) => {
    widget.applyStreamDelta(delta);
  });
  state.onClose((reason) => {
    // Never silently freeze: a dropped state feed is a stale-UI signal.
    console.error(`chat state feed closed: ${reason}`);
    setStatus(`state feed CLOSED: ${reason}`, true);
  });
  try {
    await state.connect();
    setStatus(`socket open to ${config.wsUrl} — awaiting first room snapshot…`);
  } catch (err) {
    setStatus(`socket connect FAILED: ${err instanceof Error ? err.message : String(err)}`, true);
    throw err;
  }
  // Opened but no snapshot ⇒ a subscribe/snapshot problem, not a connect one.
  setTimeout(() => {
    if (!gotState) {
      setStatus(`connected to ${config.wsUrl} but NO room snapshot arrived in 4s — subscribe/snapshot issue`, true);
    }
  }, 4000);
}

main().catch((err: unknown) => {
  // Boot failure (bad config, dead core) must be visible, not a blank page.
  console.error('web chat client failed to start:', err);
  const mount = document.getElementById('app') ?? document.body;
  const pre = document.createElement('pre');
  pre.style.cssText = 'padding:24px;color:#f77;font:13px/1.5 ui-monospace,monospace;white-space:pre-wrap';
  pre.textContent = `Continuum web chat failed to start:\n\n${err instanceof Error ? err.message : String(err)}`;
  mount.replaceChildren(pre);
});
