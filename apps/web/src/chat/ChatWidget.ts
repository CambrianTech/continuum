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
    /* Styled ENTIRELY from the shared design tokens (apps/web/src/theme.css) — no
     * hardcoded colors, so a theme swap is a :root override and the same token
     * names port to other surfaces. */
    :host {
      display: grid;
      grid-template-rows: auto 1fr auto;
      height: 100%;
      font: 14px/1.45 var(--font-primary, system-ui, sans-serif);
      color: var(--content-primary, #e0e6ed);
      background: var(--widget-surface-solid, #1a1f2e);
    }
    header.room {
      display: flex;
      align-items: baseline;
      justify-content: space-between;
      padding: var(--spacing-md) var(--spacing-lg);
      border-bottom: 1px solid var(--border-subtle);
      background: var(--widget-input-area-background);
    }
    .room-name {
      font-weight: 600;
      font-size: 15px;
      color: var(--content-accent);
    }
    .room-meta {
      display: flex;
      gap: var(--spacing-sm);
      color: var(--content-secondary);
      font-size: 12px;
    }
    .room-id {
      opacity: 0.5;
      font-family: var(--font-mono);
    }
    .panels {
      display: grid;
      grid-template-columns: minmax(160px, 220px) 1fr;
      min-height: 0;
    }
    .who {
      border-right: 1px solid var(--border-subtle);
      overflow-y: auto;
      padding: var(--spacing-sm) 0;
      background: var(--sidebar-background);
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
      padding: 5px var(--spacing-md);
    }
    .member .dot {
      width: 7px;
      height: 7px;
      border-radius: 50%;
      background: var(--status-offline);
      flex: none;
    }
    .member.active .dot {
      background: var(--status-online);
      box-shadow: 0 0 6px var(--status-online);
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
      border-radius: var(--radius-md);
      background: var(--button-secondary-background);
      color: var(--content-accent);
      border: 1px solid var(--border-accent);
    }
    .what {
      overflow-y: auto;
      padding: var(--spacing-md) var(--spacing-lg);
    }
    .empty {
      color: var(--content-secondary);
      padding: var(--spacing-xl) var(--spacing-xs);
      text-align: center;
    }
    .messages .msg {
      display: flex;
      gap: var(--spacing-sm);
      padding: 6px 0;
    }
    .msg-glyph {
      flex: none;
    }
    .msg-head {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
    }
    .sender {
      font-weight: 600;
    }
    .time {
      color: var(--content-secondary);
      font-size: 11px;
    }
    .content {
      white-space: pre-wrap;
      word-break: break-word;
      background: var(--message-assistant-background);
      border: 1px solid var(--message-assistant-border);
      border-radius: var(--radius-lg);
      padding: var(--spacing-sm) var(--spacing-md);
      margin-top: 3px;
    }
    form.compose {
      display: flex;
      gap: var(--spacing-sm);
      padding: var(--spacing-md) var(--spacing-lg);
      border-top: 1px solid var(--border-subtle);
      background: var(--widget-input-area-background);
    }
    input {
      flex: 1;
      padding: var(--spacing-sm) var(--spacing-md);
      border: 1px solid var(--input-border);
      border-radius: var(--radius-lg);
      background: var(--input-background);
      color: var(--input-text);
      font: inherit;
    }
    input:focus {
      outline: none;
      border-color: var(--input-border-focus);
    }
    input::placeholder {
      color: var(--input-placeholder);
    }
    button {
      padding: var(--spacing-sm) var(--spacing-lg);
      border: 0;
      border-radius: var(--radius-lg);
      background: var(--button-primary-background);
      color: var(--button-primary-text);
      font: inherit;
      font-weight: 600;
      cursor: pointer;
    }
    button[disabled] {
      opacity: 0.5;
      cursor: default;
    }
    .send-error {
      color: var(--content-error);
      font-size: 12px;
      padding: 0 var(--spacing-lg) var(--spacing-sm);
    }
    .connecting {
      display: grid;
      place-items: center;
      color: var(--content-secondary);
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
