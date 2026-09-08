/** Recorded Mind log. The existing focused-Mind refresh calls refresh(); this
 * view owns no timer and can dispatch only the read-only cognition/playback. */
import { html, nothing, type TemplateResult } from 'lit';

import type { CognitionPlaybackParams, CognitionPlaybackResult } from '@continuum/sdk-typescript';

type Result = CognitionPlaybackResult;
type Detail = NonNullable<Result['detail']>;
export type CallHeader = NonNullable<Result['page']>['entries'][number];
type Read = (params: CognitionPlaybackParams) => Promise<Result>;
const MAX_HEADERS = 120;
const key = (call: CallHeader): string => call.status === 'legacy'
  ? `legacy:${call.cursor}` : `${call.session_id}:${call.request_id}`;

/** Lifecycle headers are updates to a call, not separate pretend turns. Loading
 * an older page must never replace a known terminal with its submitted event. */
export function mergeCalls(current: readonly CallHeader[], incoming: readonly CallHeader[]): CallHeader[] {
  const calls = new Map(current.map((call) => [key(call), call]));
  for (const call of incoming) {
    const previous = calls.get(key(call));
    if (!previous || (previous.status === 'submitted' && call.status !== 'submitted')
      || (previous.status === call.status && previous.captured_at_ms <= call.captured_at_ms)) {
      calls.set(key(call), call);
    }
  }
  return [...calls.values()].sort((a, b) => a.started_at_ms - b.started_at_ms || a.request_id.localeCompare(b.request_id));
}

export class PlaybackView {
  persona?: string;
  opened = false;
  live = true;
  loading = false;
  calls: CallHeader[] = [];
  selected?: string;
  detail?: Detail;
  older?: string;
  newer?: string;
  issues: string[] = [];
  error?: string;
  private selection = 0;
  private epoch = 0;
  constructor(private readonly read: Read, private readonly changed: () => void) {}

  focus(persona: string | undefined): void {
    if (this.persona === persona) return;
    this.epoch += 1;
    this.persona = persona; this.opened = false; this.live = true;
    this.calls = []; this.selected = undefined; this.detail = undefined;
    this.older = undefined; this.newer = undefined; this.issues = []; this.error = undefined;
    this.loading = false; this.selection += 1;
  }
  async open(): Promise<void> { this.opened = true; this.changed(); await this.refresh(); }
  async refresh(): Promise<void> {
    if (this.opened && this.live) await this.page(false);
  }
  async page(older: boolean): Promise<void> {
    const persona = this.persona;
    const epoch = this.epoch;
    if (!persona || this.loading || (older && !this.older)) return;
    this.loading = true; this.error = undefined; this.changed();
    const cursor = older ? this.older : this.newer;
    try {
      const result = await this.read({ persona_id: persona, limit: 30,
        ...(cursor ? { cursor, newer: !older } : {}) });
      if (persona !== this.persona || epoch !== this.epoch || !result.page) return;
      const page = result.page;
      const selectedBefore = this.calls.find((call) => key(call) === this.selected)?.cursor;
      const merged = mergeCalls(this.calls, page.entries);
      // Bound retained UI state in both directions. A dropped side is recoverable
      // by paging, rather than becoming an unbounded in-browser transcript.
      this.calls = older ? merged.slice(0, MAX_HEADERS) : merged.slice(-MAX_HEADERS);
      if (older || !cursor) this.older = page.older ?? undefined;
      if (!older) this.newer = page.newer ?? this.newer;
      this.issues = page.issues;
      if (merged.length > MAX_HEADERS) this.issues = [...this.issues,
        'Showing a window of 120 calls. Use Older or Live to move through retained history.'];
      if (merged.length > MAX_HEADERS && !older) this.older = this.calls[0]?.cursor;
      const latest = this.calls.at(-1);
      if (!older && this.live) this.selected = latest ? key(latest) : undefined;
      const chosen = this.calls.find((call) => key(call) === this.selected);
      if (chosen && (!this.detail || selectedBefore !== chosen.cursor)) await this.select(chosen);
    } catch (error) {
      if (persona === this.persona && epoch === this.epoch) this.error = String(error);
    } finally {
      if (persona === this.persona && epoch === this.epoch) { this.loading = false; this.changed(); }
    }
  }
  async select(call: CallHeader): Promise<void> {
    const persona = this.persona;
    if (!persona) return;
    const selection = ++this.selection;
    this.selected = key(call); this.detail = undefined; this.error = undefined; this.changed();
    try {
      const result = await this.read({ persona_id: persona, selected: call.cursor });
      if (persona === this.persona && selection === this.selection) this.detail = result.detail ?? undefined;
    } catch (error) {
      if (persona === this.persona && selection === this.selection) this.error = String(error);
    } finally { this.changed(); }
  }
  async step(delta: number): Promise<void> {
    const index = this.calls.findIndex((call) => key(call) === this.selected);
    const call = this.calls[index + delta];
    if (call) { this.live = false; await this.select(call); }
  }
  pause(): void { this.live = false; this.changed(); }
  async resume(): Promise<void> {
    // Invalidate both an older-page read and a selected-payload read. A new
    // live request must not be suppressed by the previous mode's loading bit.
    this.epoch += 1; this.selection += 1; this.loading = false;
    this.live = true; this.selected = undefined; this.detail = undefined;
    // Reload the current tail: an older-page selection may have evicted it.
    this.newer = undefined; this.calls = []; await this.page(false);
  }
}
let view: PlaybackView | undefined;
export function configurePlayback(read: Read, changed: () => void): void { view = new PlaybackView(read, changed); }
export function focusPlayback(persona: string | undefined): void { view?.focus(persona); }
export async function refreshPlayback(persona: string): Promise<void> {
  if (view?.persona === persona) await view.refresh();
}
export function openPlayback(): void { void view?.open(); }

function object(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown> : undefined;
}
function pretty(value: unknown): string { return JSON.stringify(value, null, 2) ?? ''; }
function text(value: unknown): string { return typeof value === 'string' ? value : pretty(value); }
// The existing Mind refresh also updates vitals. Reuse the selected payload's
// rendered projection rather than stringify its unchanged transcript per pulse.
const detailTemplates = new WeakMap<Detail, TemplateResult>();
function detailView(detail: Detail): TemplateResult {
  const cached = detailTemplates.get(detail);
  if (cached) return cached;
  const submitted = object(detail.submitted);
  const request = object(submitted?.request) ?? (detail.header.status === 'legacy' ? submitted : undefined);
  const terminal = object(detail.terminal);
  const response = object(terminal?.response) ?? object(submitted?.response);
  const reasoning = typeof response?.reasoning === 'string' && response.reasoning.trim()
    ? response.reasoning : undefined;
  const messages = Array.isArray(request?.messages) ? request.messages : [];
  const header = detail.header;
  const pending = header.status === 'submitted';
  const rendered = html`<article class="mind-playback-detail" aria-label="Recorded model call">
    <div class="mp-provenance">
      <span>${header.model ?? 'Model not recorded'}</span>
      <span>Room <code>${header.room_id}</code></span>
      <span>Cycle ${header.cycle_id ?? 'not recorded'}</span>
      ${submitted?.context_window !== undefined ? html`<span>Served window ${text(submitted.context_window)} tokens</span>` : nothing}
      <span>Cause ${header.cause}</span>
      <span>Request <code>${header.request_id}</code></span>
      ${header.cause_root ? html`<span>Source <code>${header.cause_root}</code></span>` : nothing}
      ${header.status === 'legacy' ? html`<span>Legacy capture · session, cycle and source provenance missing</span>` : nothing}
      ${terminal?.elapsed_ms !== undefined ? html`<span>${text(terminal.elapsed_ms)} ms recorded</span>` : nothing}
      ${response?.requestId ? html`<span>Provider request <code>${text(response.requestId)}</code></span>` : nothing}
    </div>
    ${detail.issues.map((issue) => html`<p class="mp-issue" role="status">${issue}</p>`)}
    <h4>Recorded input</h4>
    ${request ? html`
      <details class="mp-message" open><summary>System</summary><pre>${text(request.systemPrompt ?? request.system ?? '')}</pre></details>
      ${messages.map((message, index) => {
        const item = object(message);
        return html`<details class="mp-message" open><summary>${index + 1} · ${text(item?.role ?? 'message')}</summary><pre>${text(item?.content ?? message)}</pre>
          ${item?.toolCalls ? html`<pre>${pretty(item.toolCalls)}</pre>` : nothing}</details>`;
      })}
      <details class="mp-message"><summary>Exact request · schemas, sampling, adapters and media</summary><pre>${pretty(request)}</pre></details>
    ` : html`<p class="mp-issue">The recorded input is unavailable. This is an incomplete capture.</p>`}
    <h4>Recorded output</h4>
    ${response ? html`${response.error ? html`<p class="mp-issue">${pretty(response.error)}</p>` : nothing}
      <p class="mp-muted">Finish: ${text(response.finishReason ?? 'not recorded')}</p><pre class="mp-output">${text(response.text)}</pre>
      ${reasoning ? html`<details class="mp-message"><summary>Recorded reasoning</summary><pre>${reasoning}</pre></details>` : nothing}
      ${response.toolCalls ? html`<details class="mp-message" open><summary>Proposed tool calls · recorded only</summary><pre>${pretty(response.toolCalls)}</pre></details>` : nothing}
      <p class="mp-muted">Tool observations appear in subsequent recorded inputs. A proposed tool call alone does not prove execution.</p>
      ${response.timing || response.usage ? html`<details class="mp-message"><summary>Serving timing and usage</summary><pre>${pretty({ timing: response.timing, usage: response.usage })}</pre></details>` : nothing}
      <details class="mp-message"><summary>Exact response</summary><pre>${pretty(response)}</pre></details>
      ` : html`<p class=${pending ? 'mp-muted' : 'mp-issue'}>${pending
        ? 'Submitted · no terminal receipt yet. The call may be queued, running, or interrupted by process exit.'
        : text(terminal?.error ?? submitted?.error ?? 'No response recorded.')}</p>`}
  </article>`;
  detailTemplates.set(detail, rendered);
  return rendered;
}

export function renderPlayback(persona: string): TemplateResult {
  const state = view?.persona === persona ? view : undefined;
  if (!state) return html``;
  return html`<section class="mind-playback" aria-label="Recorded mind log">
    <header class="mp-heading"><div><span class="mp-eyebrow">GLASS BOX</span><h3>Recorded mind log</h3>
      <p>Actual inputs and outputs, in order. Playback executes nothing.</p></div>
      ${state.opened ? html`<button class="p-btn" @click=${() => { if (state.live) { state.pause(); } else { void state.resume(); } }}> ${state.live ? 'Pause live' : 'Resume live'}</button>`
        : html`<button class="p-btn" @click=${openPlayback}>View log</button>`}
    </header>
    ${state.opened ? html`
      <div class="mp-controls">
        <button class="p-btn" ?disabled=${!state.older || state.loading} @click=${() => { state.live = false; void state.page(true); }}>Older</button>
        <button class="p-btn" ?disabled=${state.calls.findIndex((call) => key(call) === state.selected) <= 0} @click=${() => void state.step(-1)}>Previous call</button>
        <button class="p-btn" ?disabled=${state.calls.findIndex((call) => key(call) === state.selected) >= state.calls.length - 1} @click=${() => void state.step(1)}>Next call</button>
        <span role="status">${state.loading ? 'Reading capture…' : state.live ? 'Live · uses Mind refresh' : 'Paused'}</span>
      </div>
      ${state.error ? html`<p class="mp-issue" role="alert">${state.error}</p>` : nothing}
      ${state.issues.map((issue) => html`<p class="mp-issue" role="status">${issue}</p>`)}
      <div class="mp-layout"><nav class="mp-calls" aria-label="Recorded calls">
        ${state.calls.map((call) => html`<button class="mp-call" aria-current=${key(call) === state.selected ? 'true' : 'false'}
          @click=${() => { state.live = false; void state.select(call); }}>
          <time>${new Date(call.started_at_ms).toLocaleTimeString()}</time><span class="mp-status" data-status=${call.status}>${call.status}</span>
          <span>${call.model ?? 'Model not recorded'}</span><code>${call.request_id}</code>
        </button>`)}
      </nav><div class="mp-record">${state.detail ? detailView(state.detail)
        : html`<p class="mp-muted">${state.calls.length ? 'Select a call to read its recorded input and output.' : 'No indexed calls captured yet. Older files or incomplete capture indexes are reported above.'}</p>`}</div></div>
      <p class="mp-muted">Recorded playback preserves the original call. The separate cognition/replay command runs new inference with current faculties; it is not deterministic playback.</p>
    ` : nothing}
  </section>`;
}
