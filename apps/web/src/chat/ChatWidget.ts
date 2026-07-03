/**
 * `<chat-widget>` — the Lit custom element hosting the three-panel chat surface.
 *
 * It is deliberately thin. It owns exactly two things a pure function can't:
 *   1. the reactive `state` (a `ChatState` snapshot pushed in on each envelope),
 *      which Lit re-renders on assignment; and
 *   2. the compose bar's transient input + the send action.
 * Everything else — every "how it reads" decision — is delegated: the snapshot
 * is projected by `chatViewModel` and drawn by `renderChat`, both pure and
 * unit-tested without a browser. The widget is its own Lit host (its reactive
 * render IS the commit), so no external host/commit machinery is needed.
 *
 * Transport-agnostic on purpose: the element never imports the SDK. The entry
 * (`src/index.ts`) wires a `StateConnection` into `state` and a send callback
 * into `sendHandler`, so the widget stays a view and the wiring stays testable
 * in isolation ([[headless-core-many-clients]]).
 */

import { LitElement, html, css, nothing, type PropertyValues, type TemplateResult } from 'lit';
import type { ChatState } from '@continuum/chat-view';
import { chatViewModel } from '@continuum/chat-view';
import { renderChat } from './renderChat';

/** The send action the host injects. Resolves when the message is accepted by
 *  the core; rejects (fails loud) on a transport/command error the widget shows. */
export type SendHandler = (text: string) => Promise<void>;

export class ChatWidget extends LitElement {
  static override properties = {
    state: { attribute: false },
    sendHandler: { attribute: false },
    _draft: { state: true },
    _sending: { state: true },
    _sendError: { state: true },
  };

  /** The current chat snapshot; assignment triggers a re-render. `undefined`
   *  until the first state envelope arrives (the honest "connecting" phase). */
  state?: ChatState;

  /** Injected by the host — how a composed message reaches the core. */
  sendHandler?: SendHandler;

  private _draft = '';
  private _sending = false;
  private _sendError = '';

  static override styles = css`
    :host {
      display: grid;
      grid-template-rows: auto 1fr auto;
      height: 100%;
      font: 14px/1.4 system-ui, sans-serif;
      color: #e8e8ea;
      background: #16161a;
    }
    header.room {
      display: flex;
      align-items: baseline;
      justify-content: space-between;
      padding: 10px 14px;
      border-bottom: 1px solid #2a2a30;
    }
    .room-name {
      font-weight: 600;
      font-size: 15px;
    }
    .room-meta {
      display: flex;
      gap: 10px;
      color: #9a9aa2;
      font-size: 12px;
    }
    .room-id {
      opacity: 0.5;
      font-family: ui-monospace, monospace;
    }
    .panels {
      display: grid;
      grid-template-columns: minmax(160px, 220px) 1fr;
      min-height: 0;
    }
    .who {
      border-right: 1px solid #2a2a30;
      overflow-y: auto;
      padding: 8px 0;
    }
    ul {
      list-style: none;
      margin: 0;
      padding: 0;
    }
    .member {
      display: flex;
      align-items: center;
      gap: 6px;
      padding: 5px 12px;
    }
    .member .dot {
      width: 7px;
      height: 7px;
      border-radius: 50%;
      background: #444;
      flex: none;
    }
    .member.active .dot {
      background: #38c172;
    }
    .member.idle {
      opacity: 0.55;
    }
    .member .name {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .runtime {
      font-size: 10px;
      padding: 1px 5px;
      border-radius: 6px;
      background: #2a2a30;
      color: #b7b7c0;
    }
    .what {
      overflow-y: auto;
      padding: 10px 14px;
    }
    .empty {
      color: #77777f;
      padding: 24px 4px;
      text-align: center;
    }
    .messages .msg {
      display: flex;
      gap: 8px;
      padding: 6px 0;
    }
    .msg-glyph {
      flex: none;
    }
    .msg-head {
      display: flex;
      align-items: baseline;
      gap: 8px;
    }
    .sender {
      font-weight: 600;
    }
    .time {
      color: #77777f;
      font-size: 11px;
    }
    .content {
      white-space: pre-wrap;
      word-break: break-word;
    }
    form.compose {
      display: flex;
      gap: 8px;
      padding: 10px 14px;
      border-top: 1px solid #2a2a30;
    }
    input {
      flex: 1;
      padding: 8px 10px;
      border: 1px solid #2a2a30;
      border-radius: 8px;
      background: #1e1e24;
      color: inherit;
      font: inherit;
    }
    button {
      padding: 8px 14px;
      border: 0;
      border-radius: 8px;
      background: #4a6cf7;
      color: white;
      font: inherit;
      cursor: pointer;
    }
    button[disabled] {
      opacity: 0.5;
      cursor: default;
    }
    .send-error {
      color: #f77;
      font-size: 12px;
      padding: 0 14px 8px;
    }
    .connecting {
      display: grid;
      place-items: center;
      color: #77777f;
    }
  `;

  override render(): TemplateResult {
    if (!this.state) {
      return html`<div class="connecting">Connecting to the room…</div>`;
    }
    const vm = chatViewModel(this.state);
    return html`
      ${renderChat(vm)}
      ${this._sendError ? html`<div class="send-error">${this._sendError}</div>` : nothing}
      <form class="compose" @submit=${this.onSubmit}>
        <input
          type="text"
          placeholder="Message ${vm.roomName}…"
          .value=${this._draft}
          @input=${this.onInput}
          ?disabled=${this._sending}
          aria-label="message"
        />
        <button type="submit" ?disabled=${this._sending || this._draft.trim().length === 0}>
          ${this._sending ? 'Sending…' : 'Send'}
        </button>
      </form>
    `;
  }

  /** Keep the compose input from scrolling on every state push. */
  protected override updated(changed: PropertyValues): void {
    if (changed.has('state')) this.scrollToLatest();
  }

  private onInput = (e: Event): void => {
    this._draft = (e.target as HTMLInputElement).value;
  };

  private onSubmit = async (e: Event): Promise<void> => {
    e.preventDefault();
    const text = this._draft.trim();
    if (text.length === 0 || this._sending) return;
    if (!this.sendHandler) {
      // Fail loud: a compose with no wired send is a wiring bug, not a no-op
      // ([[fallbacks-are-illegal-fail-loud]]).
      throw new Error('<chat-widget>: submit with no sendHandler wired — the host must set it.');
    }
    this._sending = true;
    this._sendError = '';
    try {
      await this.sendHandler(text);
      this._draft = '';
    } catch (err) {
      // Surface the failure in-UI; never silently drop the user's message.
      this._sendError = `Send failed: ${err instanceof Error ? err.message : String(err)}`;
    } finally {
      this._sending = false;
    }
  };

  private scrollToLatest(): void {
    const what = this.renderRoot.querySelector('.what');
    if (what) what.scrollTop = what.scrollHeight;
  }
}

customElements.define('chat-widget', ChatWidget);

declare global {
  interface HTMLElementTagNameMap {
    'chat-widget': ChatWidget;
  }
}
