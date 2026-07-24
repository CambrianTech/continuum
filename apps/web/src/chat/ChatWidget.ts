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
import { chatViewModel, type MessageRowVM } from '@continuum/chat-view';
import type { NavViewState, StreamDelta, SystemMetricsViewState } from '@continuum/sdk-typescript';
import { renderChat } from './renderChat';
import {
  LISTING_SELECT,
  MESSAGE_EXPAND_TOGGLE,
  PANEL_RESIZE_START,
  roomSelectTarget,
  type ListingSelectDetail,
  type MessageExpandToggleDetail,
  type PanelResizeStartDetail,
} from '../render/parts';
import type { WorkspaceLayout } from '@continuum/patterns';
import '../render/CosmosBackdrop'; // registers <cosmos-backdrop> for the cosmos universe

/** The send action the host injects. Resolves when the message is accepted by
 *  the core; rejects (fails loud) on a transport/command error the widget shows. */
export type SendHandler = (text: string) => Promise<void>;

/** The room-switch action the host injects (dispatches `nav/select` through the
 *  command client — the widget stays SDK-free). Resolves when the core accepted
 *  the select; the VIEW moves only when the refocused chat/nav envelopes stream
 *  back — substrate truth only, no optimistic local active state. */
export type SelectRoomHandler = (roomId: string) => Promise<void>;

export class ChatWidget extends LitElement {
  static override properties = {
    state: { attribute: false },
    nav: { attribute: false },
    sys: { attribute: false },
    version: { attribute: false },
    sendHandler: { attribute: false },
    selectRoomHandler: { attribute: false },
    _draft: { state: true },
    _sending: { state: true },
    _sendError: { state: true },
    _selectError: { state: true },
    _typing: { state: true },
    _expanded: { state: true },
  };

  /** The current chat snapshot; assignment triggers a re-render. `undefined`
   *  until the first state envelope arrives (the honest "connecting" phase). */
  state?: ChatState;

  /** The citizen's live `kind="nav"` view (room set + unread), when the host's
   *  nav subscription has delivered. `undefined` = the rooms rail honestly shows
   *  only the focused room. */
  nav?: NavViewState;

  /** The node's live `kind="system-metrics"` view (CPU/MEM window), when the
   *  host's subscription has delivered. `undefined` = no SYS gauge, honest. */
  sys?: SystemMetricsViewState;

  /** The client build's version string (a real manifest/build stamp injected by
   *  the host) — drives the continuon header's version badge. `undefined` = no
   *  badge, honest. */
  version?: string;

  /** Injected by the host — how a composed message reaches the core. */
  sendHandler?: SendHandler;

  /** Injected by the host — how a rooms-rail pick reaches the core (`nav/select`). */
  selectRoomHandler?: SelectRoomHandler;

  private _draft = '';
  private _sending = false;
  private _sendError = '';
  private _selectError = '';
  /** #170 live typing: senderId → accumulated in-progress turn text. Ephemeral —
   *  the durable message (via `state`) supersedes it; reassigned (not mutated) so
   *  Lit re-renders. */
  private _typing = new Map<string, string>();
  /** Digest-tier expand state ([[perception-resolution-contract]]): the message
   *  ids the reader expanded to full fidelity. Widget-owned presentation state —
   *  the projection classifies, the reader chooses. Reassigned (not mutated) so
   *  Lit re-renders; toggled by the row's bubbled MESSAGE_EXPAND_TOGGLE event. */
  private _expanded = new Set<string>();

  /** Toggle one message between digest and full — the row's affordance bubbles
   *  the composed event up here because the render fragments are stateless. */
  private onExpandToggle = (e: Event): void => {
    const { id } = (e as CustomEvent<MessageExpandToggleDetail>).detail;
    const next = new Set(this._expanded);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    this._expanded = next;
  };

  /** Per-user workspace layout (column widths) — the neutral `WorkspaceLayout`
   *  shape, persisted locally per user until the airc per-(user,scope) layout
   *  row lands (same migration path as the nav focus store). Applied as host
   *  CSS vars so the grid reads it with zero re-render cost during drag. */
  private _layout: WorkspaceLayout = {};

  /** localStorage key for the layout slice. Versioned so a shape change can
   *  migrate instead of silently misparsing. */
  private static readonly LAYOUT_KEY = 'continuum.workspace.layout.v1';

  /** Clamp bounds per panel — the target's sanity rails over the neutral
   *  intent value ([[WorkspaceLayout]]: "the value is intent, not law"). */
  private static readonly PANEL_BOUNDS = {
    who: { min: 210, max: 420 },
    context: { min: 180, max: 380 },
  } as const;

  private applyLayout(): void {
    const { whoWidth, contextWidth } = this._layout;
    if (whoWidth) this.style.setProperty('--who-w', `${whoWidth}px`);
    if (contextWidth) this.style.setProperty('--ctx-w', `${contextWidth}px`);
  }

  private loadLayout(): void {
    try {
      const raw = localStorage.getItem(ChatWidget.LAYOUT_KEY);
      if (raw) this._layout = JSON.parse(raw) as WorkspaceLayout;
    } catch {
      // Corrupt layout state is presentation-only — discard, never crash.
      this._layout = {};
    }
    this.applyLayout();
  }

  /** Drag a column handle: track the pointer globally (works identically for
   *  mouse and touch/iPad — pointer events), clamp, apply live via CSS var,
   *  persist once on release. */
  private onPanelResizeStart = (e: Event): void => {
    const { panel, startX } = (e as CustomEvent<PanelResizeStartDetail>).detail;
    const bounds = ChatWidget.PANEL_BOUNDS[panel];
    const varName = panel === 'who' ? '--who-w' : '--ctx-w';
    const key = panel === 'who' ? 'whoWidth' : 'contextWidth';
    const startWidth =
      this._layout[key] ?? (panel === 'who' ? 280 : 220);
    const move = (ev: PointerEvent): void => {
      // The left rail grows rightward; the context panel grows leftward.
      const delta = panel === 'who' ? ev.clientX - startX : startX - ev.clientX;
      const next = Math.round(
        Math.min(bounds.max, Math.max(bounds.min, startWidth + delta)),
      );
      this.style.setProperty(varName, `${next}px`);
      this._layout = { ...this._layout, [key]: next };
    };
    const up = (): void => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      window.removeEventListener('pointercancel', up);
      try {
        localStorage.setItem(ChatWidget.LAYOUT_KEY, JSON.stringify(this._layout));
      } catch {
        // Storage full/blocked — the live drag still applied; persistence is
        // best-effort presentation state.
      }
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    window.addEventListener('pointercancel', up);
  };

  /** A listing cell was picked. Rooms-rail picks dispatch through the injected
   *  `selectRoomHandler` (`nav/select`); the active cell + center pane move when
   *  the refocused envelopes stream back — never an optimistic local switch. */
  private onListingSelect = (e: Event): void => {
    const target = roomSelectTarget((e as CustomEvent<ListingSelectDetail>).detail);
    if (target === null) return;
    if (!this.selectRoomHandler) {
      // Fail loud: a selectable rooms rail with no wired switch is a wiring
      // bug, not a no-op ([[fallbacks-are-illegal-fail-loud]]).
      throw new Error(
        '<chat-widget>: room select with no selectRoomHandler wired — the host must set it.',
      );
    }
    this._selectError = '';
    void this.selectRoomHandler(target).catch((err: unknown) => {
      // Surface the failure in-UI; never a silently-dead click.
      this._selectError = `Room switch failed: ${err instanceof Error ? err.message : String(err)}`;
    });
  };

  /**
   * Apply one live token from a persona's in-progress turn (#170). Grows a transient
   * "typing" bubble keyed by sender; `done` retires it. Deltas for other rooms are
   * ignored. Never touches `state` — the authoritative message still arrives there.
   */
  applyStreamDelta(delta: StreamDelta): void {
    if (delta.roomId !== this.state?.room_id) return;
    const next = new Map(this._typing);
    if (delta.done) {
      next.delete(delta.senderId);
    } else {
      next.set(delta.senderId, (next.get(delta.senderId) ?? '') + delta.token);
    }
    this._typing = next;
  }

  override connectedCallback(): void {
    super.connectedCallback();
    // The UNIVERSE axis: ?universe=<key> re-embodies the SAME chatApp inside a whole
    // experience — Tron's neon grid, a fantasy forge, a corporate onboarding — theme +
    // lore, one definition, every citizen ([[universe-is-an-experience-not-a-theme]]).
    // The target maps the key to a skin; here it's a data-attribute the styles key off.
    // Unset → the native 'continuum' look.
    const universe = new URLSearchParams(location.search).get('universe');
    if (universe) this.setAttribute('data-universe', universe);
    // Digest expand/collapse: the row's affordance fires a composed event that
    // bubbles out of the shadow tree to the host — listen on self so the pure
    // fragments need no callback threading through the render registries.
    this.addEventListener(MESSAGE_EXPAND_TOGGLE, this.onExpandToggle);
    // Column resize: handles fire the composed start event; the widget owns
    // the drag tracking + the persisted WorkspaceLayout (same pattern).
    this.addEventListener(PANEL_RESIZE_START, this.onPanelResizeStart);
    this.loadLayout();
    // Rooms-rail picks: the cell's composed LISTING_SELECT bubbles up here.
    this.addEventListener(LISTING_SELECT, this.onListingSelect);
  }

  override disconnectedCallback(): void {
    this.removeEventListener(MESSAGE_EXPAND_TOGGLE, this.onExpandToggle);
    this.removeEventListener(LISTING_SELECT, this.onListingSelect);
    super.disconnectedCallback();
  }

  static override styles = css`
    /* Styled ENTIRELY from the shared design tokens (apps/web/src/theme.css) — no
     * hardcoded colors, so a theme swap is a :root override and the same token
     * names port to other surfaces. */
    :host {
      /* Flex column (not a fixed grid-row count): the surface's row set varies —
         header, optional TAB BAR, panels, optional error strips, compose — and
         flex lets each optional row slot in while .panels keeps the remainder. */
      display: flex;
      flex-direction: column;
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
      align-items: center;
      gap: var(--spacing-sm);
      color: var(--content-secondary);
      font-size: 12px;
    }
    /* A live pulse beats a raw UUID dumped in the header (the id lives in the tooltip). */
    .live {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      font-size: 10px;
      color: var(--content-secondary);
    }
    .live-dot {
      width: 6px;
      height: 6px;
      border-radius: 50%;
      background: var(--status-online, #3fb950);
      box-shadow: 0 0 6px var(--status-online, #3fb950);
      animation: live-pulse 2.4s ease-in-out infinite;
    }
    @keyframes live-pulse {
      0%, 100% { opacity: 1; }
      50% { opacity: 0.35; }
    }
    .panels {
      display: grid;
      /* Column widths are the per-user WorkspaceLayout intent (host CSS vars,
         set by the widget's drag handling + persisted) over target defaults;
         the slim tracks between are the drag handles. */
      grid-template-columns: var(--who-w, 280px) 6px 1fr;
      min-height: 0;
      flex: 1;
    }
    /* A populated ContextPanel opens the third column — the right contextual
       rail (participants summary, room info; the factory reference's right panel). */
    .panels[data-context] {
      grid-template-columns: var(--who-w, 280px) 6px 1fr 6px var(--ctx-w, 220px);
    }
    /* Column drag handle — invisible until hover/drag, a full-height slim hit
       target (6px track, wider invisible hit area via padding-box trick is
       unnecessary at 6px on touch: iPadOS pointer coalescing hits it fine). */
    .col-handle {
      cursor: col-resize;
      touch-action: none;
      background: transparent;
      transition: background 0.15s ease;
    }
    .col-handle:hover,
    .col-handle:active {
      background: var(--content-accent);
      opacity: 0.35;
    }
    .col-handle:focus-visible {
      outline: 2px solid var(--content-accent);
      outline-offset: -2px;
    }
    .context {
      border-left: 1px solid var(--border-subtle);
      overflow-y: auto;
      padding: var(--spacing-sm) 0;
      background: var(--sidebar-background);
    }
    /* TAB BAR — the open activities as icon+title tabs (the reference's tabbed
       center). A tab pick IS the rooms-rail select (same composed event). */
    .tab-bar {
      display: flex;
      gap: 3px;
      padding: 4px var(--spacing-md) 0;
      border-bottom: 1px solid var(--border-subtle);
      background: var(--widget-input-area-background);
      overflow-x: auto;
    }
    .tab {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      padding: 4px 8px 5px 10px;
      border: 1px solid var(--border-subtle);
      border-bottom: none;
      border-radius: 7px 7px 0 0;
      font-size: 12px;
      color: var(--content-secondary);
      cursor: pointer;
      white-space: nowrap;
      user-select: none;
    }
    .tab[data-status='active'] {
      background: var(--widget-surface-solid, #1a1f2e);
      color: var(--content-primary);
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
      box-shadow: 0 -1px 8px rgba(0, 212, 255, 0.12);
    }
    .tab:hover {
      color: var(--content-primary);
    }
    .tab:focus-visible {
      outline: 1px solid var(--content-accent);
      outline-offset: -1px;
    }
    .tab-icon {
      font-size: 11px;
    }
    .tab-close {
      border: none;
      background: transparent;
      color: var(--content-secondary);
      font-size: 12px;
      line-height: 1;
      padding: 0 1px;
    }
    .tab-close[disabled] {
      opacity: 0.4;
      cursor: default;
    }
    /* Top-right header controls: version badge + Theme (real universe cycle) +
       the not-yet-wired chrome rendered honestly disabled. */
    .header-controls {
      display: inline-flex;
      gap: 4px;
      align-items: center;
    }
    .header-version {
      align-self: center;
    }
    .hdr-btn {
      padding: 2px 9px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: var(--button-secondary-background);
      color: var(--content-secondary);
      font-size: 10.5px;
      font-weight: 600;
      cursor: pointer;
      line-height: 1.6;
    }
    .hdr-btn:hover:not([disabled]) {
      color: var(--content-accent);
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
    }
    .hdr-btn[disabled] {
      opacity: 0.45;
      cursor: default;
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
    /* WHO panel header — the old "Users & Agents (N)" label. */
    .who-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: var(--spacing-sm) var(--spacing-md) var(--spacing-xs);
      text-transform: uppercase;
      letter-spacing: 0.06em;
      font-size: 11px;
      font-weight: 700;
      color: var(--content-secondary);
    }
    .who-count {
      min-width: 18px;
      padding: 0 5px;
      text-align: center;
      border-radius: var(--radius-lg);
      background: var(--button-secondary-background);
      color: var(--content-accent);
      font-size: 10px;
    }
    /* One stacked global widget in the left rail (Metrics · Rooms · Users & Agents).
     * The rail is a vertical stack; a hairline separates each widget module. Draggable
     * heights + reorder land in task #185 (this is the static stack it resizes). */
    .rail-widget {
      display: block;
      border-bottom: 1px solid var(--border-subtle);
    }
    .rail-widget:last-child {
      border-bottom: none;
    }
    /* Continuon header — the sidebar's identity mark: breathing orb + wordmark +
     * version chip + the tiny live-activity ticker (the old header's scrolling log). */
    .continuon {
      display: flex;
      align-items: center;
      gap: var(--spacing-sm);
      padding: var(--spacing-sm) var(--spacing-md);
      min-height: 44px;
    }
    .continuon-orb {
      width: 14px;
      height: 14px;
      border-radius: 50%;
      flex: none;
      background: radial-gradient(circle at 35% 35%, #7df2c8, #0f7a54 65%, #063325);
      box-shadow: 0 0 8px rgba(63, 185, 80, 0.55), inset 0 0 3px rgba(255, 255, 255, 0.35);
    }
    @keyframes continuon-breathe {
      0%, 100% { box-shadow: 0 0 5px rgba(63, 185, 80, 0.35), inset 0 0 3px rgba(255, 255, 255, 0.3); }
      50% { box-shadow: 0 0 13px rgba(63, 185, 80, 0.85), inset 0 0 4px rgba(255, 255, 255, 0.5); }
    }
    .continuon-orb[data-alive='yes'] {
      animation: continuon-breathe 3.2s ease-in-out infinite;
    }
    .continuon-orb[data-alive='no'] {
      filter: grayscale(0.8);
      opacity: 0.6;
    }
    @media (prefers-reduced-motion: reduce) {
      .continuon-orb[data-alive='yes'] {
        animation: none;
      }
    }
    .continuon-id {
      min-width: 0;
      flex: none;
    }
    .continuon-row {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
    }
    .continuon-wordmark {
      font-size: 16px;
      font-weight: 700;
      letter-spacing: 0.02em;
      color: var(--content-accent);
      text-shadow: 0 0 8px rgba(0, 212, 255, 0.35);
    }
    .continuon-version {
      font-family: var(--font-mono);
      font-size: 8px;
      padding: 1px 5px;
      border: 1px solid var(--border-accent, rgba(0, 212, 255, 0.4));
      border-radius: var(--radius-sm);
      color: var(--content-accent);
      background: rgba(0, 212, 255, 0.08);
      font-variant-numeric: tabular-nums;
    }
    .continuon-tagline {
      font-size: 8.5px;
      letter-spacing: 0.06em;
      color: var(--content-secondary);
      white-space: nowrap;
    }
    /* The live log ticker — right-aligned column of the last turns, log-tail style. */
    .continuon-ticker {
      flex: 1;
      min-width: 0;
      align-self: stretch;
      display: flex;
      flex-direction: column;
      justify-content: center;
      gap: 1px;
      overflow: hidden;
      border-left: 1px solid var(--border-subtle);
      padding-left: var(--spacing-sm);
    }
    .continuon-tick {
      font-family: var(--font-mono);
      font-size: 7.5px;
      line-height: 1.35;
      color: var(--content-secondary);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .continuon-tick:last-child {
      color: var(--content-accent);
      opacity: 0.85;
    }
    /* AI Performance widget — the live team-cognition stat row (HERE / THINKING /
     * GENOME), the honest roster-derived slice of the old AI PERFORMANCE panel. */
    .metrics-row {
      display: flex;
      gap: var(--spacing-xs);
      padding: 2px var(--spacing-md) var(--spacing-sm);
    }
    .metric {
      display: flex;
      flex-direction: column;
      align-items: flex-start;
      flex: 1;
      padding: var(--spacing-xs) var(--spacing-sm);
      border-radius: var(--radius-sm);
      background: var(--widget-surface, rgba(255, 255, 255, 0.03));
    }
    .metric-val {
      font-size: 18px;
      font-weight: 700;
      font-variant-numeric: tabular-nums;
      line-height: 1.1;
      color: var(--content-primary);
    }
    .metric-label {
      font-size: 9px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--content-secondary);
    }
    .metric[data-tone='good'] .metric-val {
      color: var(--status-online, #3fb950);
    }
    .metric[data-tone='accent'] .metric-val {
      color: var(--content-accent);
    }
    .metric[data-tone='muted'] .metric-val {
      color: var(--content-secondary);
    }
    /* SYS|AI face chips + honest window chip on the <sys-panel> header. */
    sys-panel {
      display: block;
    }
    .face-chips {
      display: inline-flex;
      gap: 2px;
      margin-left: auto;
      margin-right: var(--spacing-sm);
    }
    .face-chip {
      padding: 1px 7px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: transparent;
      color: var(--content-secondary);
      font-size: 8.5px;
      font-weight: 700;
      letter-spacing: 0.06em;
      cursor: pointer;
      line-height: 1.5;
    }
    .face-chip:hover:not([disabled]) {
      color: var(--content-primary);
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
    }
    .face-chip[data-active] {
      background: var(--content-accent);
      border-color: var(--content-accent);
      color: var(--surface, #0b0d12);
    }
    .face-chip[disabled] {
      opacity: 0.4;
      cursor: default;
    }
    .gauge-window {
      /* Keep the derived unit honest: "3m" is minutes; uppercase would read as months. */
      text-transform: none;
      font-family: var(--font-mono);
      font-size: 8.5px;
      padding: 1px 5px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    /* SYS gauge (brick 2) — the multi-series resource sparkline + legend. */
    .gauge {
      padding: 2px var(--spacing-md) var(--spacing-sm);
    }
    .gauge svg {
      display: block;
      width: 100%;
      height: 56px;
      border-radius: var(--radius-sm);
      background: var(--widget-surface, rgba(255, 255, 255, 0.03));
    }
    .gauge-grid {
      stroke: var(--border-subtle);
      stroke-width: 0.5;
    }
    .gauge-legend {
      display: flex;
      gap: var(--spacing-md);
      padding-top: var(--spacing-xs);
    }
    .gauge-key {
      display: flex;
      align-items: center;
      gap: 4px;
      font-size: 10px;
    }
    .gauge-dot {
      width: 6px;
      height: 6px;
      border-radius: 50%;
    }
    .gauge-label {
      letter-spacing: 0.08em;
      color: var(--content-secondary);
    }
    .gauge-val {
      font-variant-numeric: tabular-nums;
      font-weight: 700;
      color: var(--content-primary);
    }
    /* NODES strip — the factory sidebar's "1/1 nodes online": pulse dot + host
     * name + role chip per attested node. */
    .nodes-online {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      margin-left: auto;
      font-size: 9px;
      letter-spacing: 0.05em;
      color: var(--content-secondary);
    }
    ul.nodes {
      list-style: none;
      margin: 0;
      padding: 0 var(--spacing-md) var(--spacing-sm);
    }
    .node-row {
      display: flex;
      align-items: center;
      gap: var(--spacing-sm);
      padding: 2px 0;
      font-size: 12px;
      color: var(--content-primary);
    }
    .node-dot {
      width: 7px;
      height: 7px;
      border-radius: 50%;
      background: var(--status-offline, #555);
      flex: none;
    }
    .node-dot[data-on] {
      background: var(--status-online, #3fb950);
      box-shadow: 0 0 5px var(--status-online, #3fb950);
    }
    .node-name {
      font-family: var(--font-mono);
      font-size: 11px;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .node-role {
      margin-left: auto;
      font-size: 8px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      padding: 1px 6px;
      clip-path: polygon(4px 0, 100% 0, 100% 100%, 0 100%);
      background: rgba(0, 212, 255, 0.14);
      color: var(--content-accent);
    }
    /* Rooms widget — the live room set (brick 1) drawn by <rooms-panel>: filter
     * facets (All/Rooms/DMs — a facet over the neutral group key), purpose
     * descriptions under names, unread pills, and the (honestly disabled)
     * start-conversation affordance. Light-DOM element → styled from here. */
    rooms-panel {
      display: block;
    }
    .rooms-facets {
      display: inline-flex;
      gap: 2px;
      margin-left: auto;
      margin-right: var(--spacing-sm);
    }
    .rooms-facet {
      padding: 1px 7px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: transparent;
      color: var(--content-secondary);
      font-size: 8.5px;
      font-weight: 600;
      letter-spacing: 0.05em;
      cursor: pointer;
      line-height: 1.5;
    }
    .rooms-facet:hover {
      color: var(--content-primary);
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
    }
    .rooms-facet[data-active] {
      background: var(--content-accent);
      border-color: var(--content-accent);
      color: var(--surface, #0b0d12);
    }
    .rooms-empty {
      padding: var(--spacing-xs) var(--spacing-md) var(--spacing-sm);
      color: var(--content-secondary);
      font-size: 11px;
      font-style: italic;
    }
    .rooms-start {
      display: block;
      width: calc(100% - 2 * var(--spacing-md));
      margin: 2px var(--spacing-md) var(--spacing-sm);
      padding: 3px 0;
      border: 1px dashed var(--border-subtle);
      border-radius: var(--radius-sm);
      background: transparent;
      color: var(--content-secondary);
      font-size: 10.5px;
      text-align: center;
    }
    .rooms-start[disabled] {
      opacity: 0.55;
      cursor: default;
    }
    ul.cells {
      list-style: none;
      margin: 0;
      padding: 0 var(--spacing-sm) var(--spacing-sm);
    }
    .cell {
      display: flex;
      align-items: center;
      gap: var(--spacing-sm);
      padding: var(--spacing-xs) var(--spacing-sm);
      border-radius: var(--radius-sm);
      color: var(--content-secondary);
      font-size: 13px;
      cursor: default;
    }
    .cell[data-status='active'] {
      background: var(--button-secondary-background);
      color: var(--content-primary);
      border-left: 2px solid var(--content-accent);
    }
    /* Selectable cells (the rooms rail) — a pick is a nav/select round-trip; the
     * active highlight moves only when the refocused envelope arrives. */
    .cell[data-selectable] {
      cursor: pointer;
    }
    .cell[data-selectable]:hover {
      background: var(--widget-surface, rgba(255, 255, 255, 0.05));
      color: var(--content-primary);
    }
    .cell[data-selectable]:focus-visible {
      outline: 1px solid var(--content-accent);
      outline-offset: -1px;
    }
    .cell-body {
      flex: 1;
      min-width: 0;
    }
    .cell-title {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .cell-subtitle {
      font-size: 11px;
      color: var(--content-secondary);
    }
    .cell-count {
      min-width: 18px;
      padding: 0 5px;
      text-align: center;
      border-radius: var(--radius-lg);
      background: var(--content-accent);
      color: var(--surface, #0b0d12);
      font-size: 10px;
      font-weight: 700;
      font-variant-numeric: tabular-nums;
      line-height: 16px;
    }
    /* #186 LIVE COMPASS: the cognition diamond's triangles glow + fade smoothly as the
     * radiator pushes new faculty levels (~2s). This CSS transition IS the "steady glow"
     * lane — a faculty firing brightens its triangle, then it eases back toward dark.
     * The per-event FLASH (lane B) rides on top of this. Honors reduced-motion. */
    .cog-tri {
      transition: opacity 0.7s ease, fill 0.7s ease;
    }
    @media (prefers-reduced-motion: reduce) {
      .cog-tri {
        transition: none;
      }
    }
    /* ================= PERSONA TILE =================
     * The legacy persona-tile (legacy/src/widgets/chat/user-list), ported onto
     * the positron member card: avatar-in-ring + status dots · name · green-mono
     * identity chips · labelled vital meters · the GENOME instrument panel.
     * Every colour is a named theme token; spacing/type/motion ride the shared
     * scales, so a universe skin restyles the WHOLE tile by overriding tokens. */

    .member {
      position: relative;
      display: flex;
      align-items: center;
      gap: var(--spacing-sm);
      padding: var(--spacing-xs) var(--spacing-sm);
      /* Chamfered HUD module (notched corners), not a rounded row. */
      clip-path: polygon(0 0, calc(100% - 7px) 0, 100% 7px, 100% 100%, 7px 100%, 0 calc(100% - 7px));
      transition: background var(--motion-fast) var(--motion-ease);
    }
    .member.idle {
      opacity: 0.6;
    }
    .member.clickable {
      cursor: pointer;
    }
    .member:hover,
    .member.clickable:focus-visible {
      background: linear-gradient(90deg, rgba(0, 212, 255, 0.09), transparent 70%);
      outline: none;
    }
    /* HUD corner brackets on hover/focus — the framed-module affordance. */
    .member.clickable::before,
    .member.clickable::after {
      content: '';
      position: absolute;
      width: 7px;
      height: 7px;
      opacity: 0;
      transition: opacity var(--motion-fast) var(--motion-ease);
      pointer-events: none;
    }
    .member.clickable::before {
      top: 2px;
      left: 2px;
      border-top: 1px solid var(--content-accent);
      border-left: 1px solid var(--content-accent);
    }
    .member.clickable::after {
      right: 2px;
      bottom: 2px;
      border-bottom: 1px solid var(--content-accent);
      border-right: 1px solid var(--content-accent);
    }
    .member.clickable:hover::before,
    .member.clickable:hover::after,
    .member.clickable:focus-visible::before,
    .member.clickable:focus-visible::after {
      opacity: 1;
    }

    /* --- Avatar: image in a live ring, presence dot at 4 o'clock ----------- */
    .member .avatar {
      position: relative;
      width: 42px;
      height: 42px;
      border-radius: 50%;
      display: grid;
      place-items: center;
      font-size: 20px;
      flex: none;
      background: var(--border-subtle);
      border: 2px solid var(--border-subtle);
    }
    .member[data-kind='agent'] .avatar {
      border-color: var(--widget-border);
      box-shadow: 0 0 6px var(--hud-accent-glow);
    }
    .avatar-img {
      position: absolute;
      inset: 0;
      width: 100%;
      height: 100%;
      border-radius: 50%;
      object-fit: cover;
      /* Legacy spec (PersonaTile.ts:308): cover + CENTER TOP — VRM portraits
       * carry the face at the top of the frame; a centered crop lands on the
       * dark collar. Scene-lit live renders (the bevy scene compositor) are the
       * follow-up that replaces the dark field entirely. */
      object-position: center top;
    }
    /* Presence dot — the legacy status-indicator, bottom-right on the ring. */
    .member .status-dot {
      position: absolute;
      bottom: -2px;
      right: -2px;
      width: 12px;
      height: 12px;
      border-radius: 50%;
      background: var(--status-offline);
      border: 2px solid var(--widget-surface-solid);
      z-index: 2;
      transition: background var(--motion-base) var(--motion-ease),
        box-shadow var(--motion-base) var(--motion-ease);
    }
    .member.online .status-dot {
      background: var(--status-online);
      box-shadow: 0 0 5px var(--status-online);
    }
    /* Emotional-event emoji, over the avatar. */
    .emoji-overlay {
      position: absolute;
      top: -4px;
      right: -5px;
      font-size: 14px;
      line-height: 1;
      filter: drop-shadow(0 0 3px rgba(0, 0, 0, 0.85));
      z-index: 3;
    }

    /* --- The live ring: breathing at idle, comet arc while the mind works --
     * A slow "alive" breath on any online agent; the orbiting comet arc fires
     * on REAL state only — speaking (live token rail) and thinking (radiator)
     * carry their own named ring hues; error flares the border. Transform/
     * opacity-composited; reduced-motion collapses to static colour. */
    @keyframes alive-pulse {
      0%,
      100% {
        box-shadow: 0 0 5px var(--hud-accent-glow);
      }
      50% {
        box-shadow: 0 0 14px var(--border-accent);
      }
    }
    @keyframes comet-orbit {
      from {
        transform: rotate(0deg);
      }
      to {
        transform: rotate(360deg);
      }
    }
    .member[data-kind='agent'].online .avatar {
      animation: alive-pulse 3s var(--motion-ease) infinite;
    }
    .member .avatar::before {
      content: '';
      position: absolute;
      inset: -5px;
      border-radius: 50%;
      border: 3px solid transparent;
      opacity: 0;
      transition: opacity var(--motion-base) var(--motion-ease);
      pointer-events: none;
      z-index: 1;
    }
    .member .avatar[data-state='speaking']::before,
    .member .avatar[data-state='thinking']::before {
      opacity: 1;
      border-top-color: var(--comet-color);
      border-right-color: var(--comet-color);
      animation: comet-orbit 3.5s linear infinite;
    }
    .member .avatar[data-state='speaking'] {
      --comet-color: var(--ring-speaking);
      border-color: var(--ring-speaking);
    }
    .member .avatar[data-state='thinking'] {
      --comet-color: var(--ring-thinking);
      border-color: var(--ring-thinking);
    }
    .member .avatar[data-state='error'] {
      border-color: var(--content-error);
      box-shadow: 0 0 0 1px var(--content-error), 0 0 8px var(--content-error);
    }
    .member .avatar[data-state='idle'] {
      opacity: 0.7;
    }

    /* --- Identity column ---------------------------------------------------- */
    .member .info {
      display: flex;
      flex-direction: column;
      gap: 2px;
      min-width: 0;
      flex: 1;
    }
    .member .name {
      font-size: 14px;
      font-weight: 600;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .member .meta {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
    }
    /* Green-mono identity chips — the legacy tile-type-badge look: bare glowing
     * mono caps, no pill boxes (PERSONA  ALIBABA in the reference). */
    .member .kind-badge,
    .runtime {
      font-family: var(--font-mono);
      font-size: var(--font-size-xs);
      font-weight: 700;
      text-transform: uppercase;
      letter-spacing: 0.12em;
      color: var(--hud-accent-dim);
      text-shadow: 0 0 4px var(--hud-accent-glow);
    }
    /* Recency stamp — the reference's "55m ago", quiet in the top-right. */
    .member .ago {
      position: absolute;
      top: 3px;
      right: 10px;
      font-family: var(--font-mono);
      font-size: var(--font-size-xs);
      letter-spacing: 0.04em;
      color: var(--content-secondary);
      opacity: 0.75;
      pointer-events: none;
    }
    /* LOADOUT strip — the model backing the persona (model · size · ctx). */
    .member .loadout {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-xs);
      font-family: var(--font-mono);
      font-size: var(--font-size-xs);
      font-variant-numeric: tabular-nums;
      color: var(--content-secondary);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }
    .member .loadout-part:first-of-type {
      color: var(--content-accent);
      letter-spacing: 0.02em;
    }
    .member .loadout-sep {
      opacity: 0.4;
    }

    /* --- Vital meters: the legacy INT/NRG/QUE stack, live ------------------ */
    .meters {
      display: flex;
      flex-direction: column;
      gap: 2px;
      margin-top: 2px;
    }
    .meter {
      display: flex;
      align-items: center;
      gap: var(--spacing-xs);
    }
    .meter-label {
      font-family: var(--font-mono);
      font-size: var(--font-size-2xs);
      font-weight: 700;
      letter-spacing: 0.08em;
      color: var(--hud-accent-dim);
      text-shadow: 0 0 3px var(--hud-accent-glow);
      width: 22px;
      flex: none;
    }
    .meter-track {
      position: relative;
      height: 5px;
      flex: 1;
      max-width: 72px;
      border-radius: 2px;
      background: var(--hud-track-background);
      border: 1px solid var(--hud-track-border);
      overflow: hidden;
    }
    .meter-fill {
      display: block;
      height: 100%;
      border-radius: 1px;
      background: var(--meter-color, var(--content-accent));
      box-shadow: 0 0 4px var(--meter-color, var(--content-accent));
      transition: width var(--motion-base) var(--motion-ease);
      overflow: hidden;
      position: relative;
    }
    .meter-val {
      font-family: var(--font-mono);
      font-size: var(--font-size-2xs);
      color: var(--meter-color, var(--content-accent));
      width: 14px;
      text-align: right;
      flex: none;
      font-variant-numeric: tabular-nums;
    }
    /* One named hue per vital — set once on the row, read by fill + value. */
    .meter[data-key='activity'] {
      --meter-color: var(--meter-act);
    }
    .meter[data-key='queue'] {
      --meter-color: var(--meter-que);
    }
    .meter[data-key='speed'] {
      --meter-color: var(--meter-spd);
    }
    .meter[data-key='size'] {
      --meter-color: var(--meter-par);
    }
    /* Live glint across a working persona's bars — a moving gauge, game-feel. */
    @keyframes meter-shimmer {
      from {
        transform: translateX(-120%);
      }
      to {
        transform: translateX(320%);
      }
    }
    .member.online .meter-fill::after {
      content: '';
      position: absolute;
      inset: 0;
      width: 40%;
      background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.55), transparent);
      animation: meter-shimmer 2.4s linear infinite;
    }

    /* --- GENOME panel: the legacy instrument block, faithful ---------------
     * Rotated caption · four FULL-HEIGHT gene slots (dark until a gene pages
     * in — visible empty equipment slots, never half-mast bars) · the
     * cognition compass at top-right. */
    .genome-panel {
      display: grid;
      grid-template-columns: auto 1fr auto;
      column-gap: var(--spacing-xs);
      align-items: end;
      align-self: stretch;
      flex: none;
      margin-left: auto;
      /* Clears the tile's top-right "55m ago" stamp — the reference keeps the
         recency line ABOVE the panel, never over it. */
      margin-top: 13px;
      min-height: 46px;
      padding: var(--spacing-xs) 5px;
      background: var(--hud-panel-background);
      border: 1px solid var(--hud-accent-border);
      border-radius: var(--radius-md);
      box-shadow: 0 0 8px var(--hud-accent-glow);
      transition: border-color var(--motion-fast) var(--motion-ease);
    }
    .member:hover .genome-panel {
      border-color: var(--hud-accent);
    }
    .genome-label {
      font-family: var(--font-mono);
      font-size: var(--font-size-2xs);
      font-weight: 700;
      letter-spacing: 0.08em;
      color: var(--hud-accent);
      text-shadow: 0 0 4px var(--hud-accent-glow);
      writing-mode: vertical-rl;
      transform: rotate(180deg);
      line-height: 1;
      align-self: center;
    }
    .genome-slots {
      display: flex;
      gap: 2px;
      justify-content: center;
    }
    .genome-slot {
      width: 7px;
      height: 17px;
      border-radius: 1px;
      background: var(--hud-slot-background);
      border: 1px solid var(--hud-slot-border);
      transition: background var(--motion-base) var(--motion-ease),
        border-color var(--motion-base) var(--motion-ease),
        box-shadow var(--motion-base) var(--motion-ease);
    }
    /* Page-in moment: the slot IGNITES (bright flash → settle) — game-feel on a
     * real state change only (the class flips when the radiator reports the
     * gene). Opacity-composited. */
    @keyframes gene-ignite {
      0% {
        opacity: 0.2;
      }
      60% {
        opacity: 1;
      }
      100% {
        opacity: 0.92;
      }
    }
    .genome-slot.lit {
      background: var(--hud-accent);
      border-color: var(--hud-accent);
      box-shadow: 0 0 5px var(--hud-accent-glow);
      animation: gene-ignite var(--motion-slow) var(--motion-ease);
    }
    /* Cognition compass — four faculty triangles, top-right of the panel. */
    .cog-diamond {
      width: 26px;
      height: 26px;
      flex: none;
      align-self: start;
      margin: 1px 0 0 1px;
    }
    .cog-tri {
      filter: drop-shadow(0 0 1.5px rgba(255, 255, 255, 0.25));
    }

    @media (prefers-reduced-motion: reduce) {
      .member[data-kind='agent'].online .avatar,
      .member .avatar[data-state='speaking']::before,
      .member .avatar[data-state='thinking']::before,
      .member.online .meter-fill::after,
      .genome-slot.lit {
        animation: none;
      }
    }

    .what {
      overflow-y: auto;
      padding: var(--spacing-md) var(--spacing-lg);
      /* Grid items default to min-width:auto and won't shrink below their content,
         so a long message makes .what overflow its track and clip at narrow widths.
         min-width:0 lets it honor the track so the bubbles wrap within the viewport. */
      min-width: 0;
    }
    .empty {
      color: var(--content-secondary);
      padding: var(--spacing-xl) var(--spacing-xs);
      text-align: center;
    }
    /* A centered reading column — the conversation shouldn't hug the left edge of a
       wide desktop panel with a dead void on the right. */
    .messages {
      max-width: 880px;
      margin: 0 auto;
    }
    .messages .msg {
      display: flex;
      gap: var(--spacing-sm);
      padding: 6px 0;
    }
    .msg-glyph {
      flex: none;
    }
    /* The flex child holding the message text MUST be allowed to shrink below its
       content's intrinsic width, or the bubble overflows its container and clips
       (mid-word) instead of wrapping — the canonical flexbox min-width:auto trap.
       Applies at every width; the bubble should never overflow the viewport. */
    .msg-body {
      min-width: 0;
      flex: 1;
    }
    .msg-head {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
    }
    .sender {
      font-weight: 600;
      color: var(--content-accent);
    }
    .time {
      color: var(--content-secondary);
      font-size: 11px;
    }
    .content {
      white-space: pre-wrap;
      word-break: break-word;
      /* Cap the measure — prose past ~68ch is hard to read; a lovable chat never
         stretches a bubble the full 1000px+ of a desktop panel. */
      max-width: 68ch;
      background: var(--message-assistant-background);
      border: 1px solid var(--message-assistant-border);
      border-radius: var(--radius-lg);
      padding: var(--spacing-sm) var(--spacing-md);
      margin-top: 3px;
    }
    /* Digest tier ([[perception-resolution-contract]]) — a flooding message renders
       collapsed: head + a mechanical tail line + an expand affordance. No animation
       on toggle — an instant swap is simple and already honors reduced-motion. */
    .digest-tail {
      margin-top: var(--spacing-sm);
      padding-top: var(--spacing-xs);
      border-top: 1px dashed var(--border-subtle);
      font-family: var(--font-mono);
      font-size: 11px;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    .digest-histogram {
      /* The repetition callout — the named degenerate pattern, warmed so it reads
         as the anomaly signal, not more body text. */
      color: #ffb020;
    }
    button.digest-toggle {
      display: inline-block;
      margin-top: var(--spacing-xs);
      padding: 2px 10px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: var(--button-secondary-background);
      color: var(--content-secondary);
      font-size: 11px;
      font-weight: 600;
      letter-spacing: 0.04em;
      cursor: pointer;
    }
    button.digest-toggle:hover,
    button.digest-toggle:focus-visible {
      color: var(--content-accent);
      border-color: var(--border-accent);
      outline: none;
    }
    /* Fenced code + commands — personas speak these constantly; a monospace block reads
       as an action, not noise. Scrolls inside itself so a long command never widens the bubble. */
    /* Expandable code block — a summary bar you can toggle; long commands/outputs stay
       collapsed so they never bury the conversation. */
    .code-collapsible {
      margin: 7px 0 3px;
      background: rgba(0, 0, 0, 0.32);
      border: 1px solid var(--border-subtle);
      border-radius: 7px;
      overflow: hidden;
    }
    .code-collapsible summary {
      cursor: pointer;
      padding: 6px 11px;
      font-family: var(--font-mono);
      font-size: 10px;
      letter-spacing: 0.05em;
      text-transform: uppercase;
      color: var(--content-secondary);
      display: flex;
      align-items: center;
      gap: 8px;
      list-style: none;
      user-select: none;
    }
    .code-collapsible summary::-webkit-details-marker {
      display: none;
    }
    .code-collapsible summary::before {
      content: '▸';
      color: var(--content-accent);
      font-size: 9px;
    }
    .code-collapsible[open] summary::before {
      content: '▾';
    }
    .code-collapsible .code-count {
      margin-left: auto;
      opacity: 0.7;
      text-transform: none;
      letter-spacing: 0;
    }
    .code-collapsible pre {
      margin: 0;
      padding: 2px 11px 10px;
      overflow-x: auto;
      font-family: var(--font-mono);
      font-size: 12.5px;
      line-height: 1.5;
      color: var(--content-accent);
    }
    .code-collapsible pre code {
      white-space: pre;
    }
    /* highlight.js dark palette (github-dark), scoped into the shadow root. */
    .hljs {
      color: #c9d1d9;
    }
    .hljs-comment,
    .hljs-quote {
      color: #8b949e;
      font-style: italic;
    }
    .hljs-keyword,
    .hljs-selector-tag,
    .hljs-built_in,
    .hljs-name,
    .hljs-tag {
      color: #ff7b72;
    }
    .hljs-string,
    .hljs-attr,
    .hljs-template-tag,
    .hljs-addition {
      color: #a5d6ff;
    }
    .hljs-title,
    .hljs-section,
    .hljs-type {
      color: #d2a8ff;
    }
    .hljs-number,
    .hljs-literal,
    .hljs-variable,
    .hljs-selector-attr {
      color: #79c0ff;
    }
    .hljs-symbol,
    .hljs-bullet,
    .hljs-meta {
      color: #56d364;
    }
    .hljs-attribute {
      color: #ffa657;
    }
    .hljs-emphasis {
      font-style: italic;
    }
    .hljs-strong {
      font-weight: 600;
    }
    .code-block {
      margin: 7px 0 3px;
      padding: 8px 11px;
      background: rgba(0, 0, 0, 0.32);
      border: 1px solid var(--border-subtle);
      border-radius: 7px;
      overflow-x: auto;
      font-family: var(--font-mono);
      font-size: 12.5px;
      line-height: 1.5;
      color: var(--content-accent);
    }
    .code-block code {
      white-space: pre;
    }
    .inline-code {
      font-family: var(--font-mono);
      font-size: 0.88em;
      background: rgba(0, 0, 0, 0.28);
      border: 1px solid var(--border-subtle);
      border-radius: 4px;
      padding: 1px 5px;
      color: var(--content-accent);
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
      flex: 1;
      color: var(--content-secondary);
    }
    .render-error {
      padding: var(--spacing-lg);
      color: var(--content-error);
      font-family: var(--font-mono);
      font-size: 13px;
      white-space: pre-wrap;
    }

    /* The MOBILE adaptation rule (@media modality:mobile via viewport) — LAST in the
       sheet so it wins by source order (media queries add no specificity). The desktop
       three-panel is wrong on a phone, so the presentation is DERIVED, not reflowed:
       the conversation (primary) takes the screen; the roster (secondary) collapses to a
       compact horizontal "who's here" avatar strip on top; secondary per-member detail
       (kind badge, runtime, vitals) is DROPPED — a phone shows who's present, not a full
       dossier per row. Best UX for THIS portal, not the desktop shrunk. */
    @media (max-width: 720px) {
      .panels {
        grid-template-columns: 1fr;
        grid-template-rows: auto 1fr;
      }
      /* No column drag on the single-column phone layout. */
      .col-handle {
        display: none;
      }
      .who {
        border-right: none;
        border-bottom: 1px solid var(--border-subtle);
        overflow-x: auto;
        overflow-y: hidden;
      }
      .who-head {
        padding-bottom: 2px;
      }
      /* Roster reflows to a horizontal, thumb-scrollable avatar strip. */
      .roster {
        display: flex;
        flex-direction: row;
        gap: var(--spacing-md);
        padding: 2px var(--spacing-md) var(--spacing-sm);
      }
      .member {
        flex-direction: column;
        align-items: center;
        gap: 3px;
        width: 60px;
        flex: none;
      }
      .member .info {
        align-items: center;
        width: 60px;
      }
      .member .name {
        font-size: 11px;
        max-width: 60px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }
      /* Mobile drops the secondary per-member detail — presence, not a dossier. */
      .member .meta,
      .member .loadout,
      .member .meters,
      .member .ago,
      .genome-panel {
        display: none;
      }
    }

    /* ── UNIVERSE: tron ── the SAME app, re-embodied as a neon grid portal. Not a
       theme swap — an EXPERIENCE ([[universe-is-an-experience-not-a-theme]]): the grid
       floor, glowing programs, the derez cyan. One chatApp, a whole world over it. */
    :host([data-universe='tron']) {
      /* Tile HUD tokens re-skinned to the grid's electric cyan — the whole
         instrument cluster (chips, meters, genome, rings) follows. */
      --hud-accent: rgba(0, 224, 255, 0.9);
      --hud-accent-dim: rgba(0, 224, 255, 0.65);
      --hud-accent-border: rgba(0, 224, 255, 0.4);
      --hud-accent-glow: rgba(0, 224, 255, 0.35);
      --hud-panel-background: rgba(0, 14, 24, 0.9);
      --meter-act: #00e0ff;
      --meter-que: #00fff0;
      --ring-speaking: #00e0ff;
      --ring-thinking: #66f6ff;
      color: #cfefff;
      background:
        radial-gradient(ellipse 90% 60% at 50% 8%, rgba(0, 200, 255, 0.10), transparent 70%),
        repeating-linear-gradient(0deg, transparent 0 43px, rgba(0, 224, 255, 0.07) 43px 44px),
        repeating-linear-gradient(90deg, transparent 0 43px, rgba(0, 224, 255, 0.07) 43px 44px),
        linear-gradient(180deg, #00080f, #01030a);
    }
    :host([data-universe='tron']) .room {
      border-bottom: 1px solid rgba(0, 224, 255, 0.35);
      box-shadow: 0 1px 18px rgba(0, 200, 255, 0.18);
    }
    :host([data-universe='tron']) .room-name {
      color: #66f6ff;
      letter-spacing: 0.14em;
      text-transform: uppercase;
      text-shadow: 0 0 10px rgba(0, 224, 255, 0.9), 0 0 26px rgba(0, 200, 255, 0.5);
    }
    :host([data-universe='tron']) .live-dot {
      background: #00fff0;
      box-shadow: 0 0 10px #00fff0, 0 0 22px rgba(0, 255, 240, 0.6);
    }
    :host([data-universe='tron']) .who,
    :host([data-universe='tron']) form.compose {
      border-color: rgba(0, 224, 255, 0.28);
      background: rgba(0, 12, 22, 0.55);
    }
    :host([data-universe='tron']) .sender {
      color: #6ff0ff;
      text-shadow: 0 0 8px rgba(0, 224, 255, 0.7);
    }
    :host([data-universe='tron']) .content {
      background: rgba(0, 18, 30, 0.55);
      border: 1px solid rgba(0, 224, 255, 0.35);
      box-shadow: 0 0 14px rgba(0, 200, 255, 0.12), inset 0 0 10px rgba(0, 200, 255, 0.06);
      backdrop-filter: blur(1px);
    }
    /* Programs glow on the grid — the avatar ring lights up cyan. */
    :host([data-universe='tron']) .avatar {
      box-shadow: 0 0 12px rgba(0, 224, 255, 0.55);
      border-radius: 50%;
    }
    :host([data-universe='tron']) .status-dot {
      box-shadow: 0 0 8px currentColor;
    }
    :host([data-universe='tron']) .who-title,
    :host([data-universe='tron']) .name {
      color: #a9e9ff;
      text-shadow: 0 0 6px rgba(0, 200, 255, 0.4);
    }
    :host([data-universe='tron']) input {
      color: #cfefff;
    }

    /* ── UNIVERSE: forge ── the range test. Not neon — WARMTH: the orc's forge, fire
       glowing up from below, molten-amber programs, hammered iron. Same chatApp, a wholly
       different WORLD ([[universe-is-an-experience-not-a-theme]]) — proving a universe is
       an experience, not a colour swap. */
    :host([data-universe='forge']) {
      /* Tile HUD tokens in molten amber — same instruments, forge-lit. */
      --hud-accent: rgba(255, 180, 70, 0.9);
      --hud-accent-dim: rgba(255, 180, 70, 0.65);
      --hud-accent-border: rgba(255, 140, 40, 0.4);
      --hud-accent-glow: rgba(255, 130, 30, 0.3);
      --hud-panel-background: rgba(28, 16, 8, 0.9);
      --meter-act: #ffb347;
      --meter-que: #ff9a2e;
      --ring-speaking: #ffb347;
      --ring-thinking: #ff6a3d;
      color: #efdcc0;
      background:
        radial-gradient(ellipse 85% 55% at 50% 112%, rgba(255, 120, 20, 0.32), transparent 60%),
        radial-gradient(ellipse 55% 35% at 50% 104%, rgba(255, 190, 70, 0.22), transparent 52%),
        linear-gradient(180deg, #0e0a06, #17100a);
    }
    :host([data-universe='forge']) .room {
      border-bottom: 1px solid rgba(255, 140, 40, 0.4);
      box-shadow: 0 1px 22px rgba(255, 110, 20, 0.22);
    }
    :host([data-universe='forge']) .room-name {
      color: #ffb347;
      letter-spacing: 0.1em;
      text-transform: uppercase;
      text-shadow: 0 0 12px rgba(255, 130, 30, 0.85), 0 0 3px rgba(255, 210, 120, 0.9);
    }
    :host([data-universe='forge']) .live-dot {
      background: #ff9a2e;
      box-shadow: 0 0 10px #ff7a18, 0 0 22px rgba(255, 120, 20, 0.6);
    }
    :host([data-universe='forge']) .who,
    :host([data-universe='forge']) form.compose {
      border-color: rgba(255, 130, 40, 0.24);
      background: rgba(30, 18, 8, 0.5);
    }
    :host([data-universe='forge']) .sender {
      color: #ffb865;
      text-shadow: 0 0 8px rgba(255, 130, 30, 0.6);
    }
    :host([data-universe='forge']) .content {
      background: rgba(28, 18, 10, 0.6);
      border: 1px solid rgba(255, 130, 40, 0.32);
      box-shadow: 0 0 16px rgba(255, 110, 20, 0.12), inset 0 0 12px rgba(255, 90, 10, 0.06);
    }
    :host([data-universe='forge']) .avatar {
      box-shadow: 0 0 14px rgba(255, 130, 30, 0.5);
      border-radius: 50%;
    }
    :host([data-universe='forge']) .status-dot {
      box-shadow: 0 0 8px currentColor;
    }
    :host([data-universe='forge']) .who-title,
    :host([data-universe='forge']) .name {
      color: #e8c89a;
      text-shadow: 0 0 6px rgba(255, 130, 30, 0.4);
    }
    :host([data-universe='forge']) input {
      color: #efdcc0;
    }

    /* ── UNIVERSE: cosmos ── a universe that MOVES. <cosmos-backdrop> paints a living
       starfield + constellation network behind translucent glass panels, so the citizens
       converse afloat in a breathing cosmos. A world in motion, not a colour swap. */
    :host([data-universe='cosmos']) {
      position: relative;
      color: #dfe6ff;
      background: #05010f;
    }
    :host([data-universe='cosmos']) .room,
    :host([data-universe='cosmos']) .panels,
    :host([data-universe='cosmos']) form.compose {
      position: relative;
      z-index: 1;
    }
    :host([data-universe='cosmos']) .room {
      background: rgba(5, 3, 20, 0.5);
      backdrop-filter: blur(3px);
      border-bottom: 1px solid rgba(140, 160, 255, 0.25);
    }
    :host([data-universe='cosmos']) .room-name {
      color: #cfe0ff;
      letter-spacing: 0.12em;
      text-shadow: 0 0 12px rgba(120, 160, 255, 0.9);
    }
    :host([data-universe='cosmos']) .who {
      background: rgba(8, 6, 26, 0.4);
      backdrop-filter: blur(3px);
      border-color: rgba(140, 160, 255, 0.18);
    }
    :host([data-universe='cosmos']) .sender {
      color: #bcccff;
      text-shadow: 0 0 8px rgba(120, 160, 255, 0.6);
    }
    :host([data-universe='cosmos']) .content {
      background: rgba(12, 10, 34, 0.46);
      border: 1px solid rgba(150, 170, 255, 0.28);
      box-shadow: 0 0 16px rgba(90, 120, 255, 0.1);
      backdrop-filter: blur(4px);
    }
    :host([data-universe='cosmos']) form.compose {
      background: rgba(8, 6, 26, 0.55);
      backdrop-filter: blur(4px);
      border-top-color: rgba(140, 160, 255, 0.2);
    }
    :host([data-universe='cosmos']) .live-dot {
      background: #9ab8ff;
      box-shadow: 0 0 10px #9ab8ff, 0 0 22px rgba(120, 160, 255, 0.6);
    }
    :host([data-universe='cosmos']) .avatar {
      box-shadow: 0 0 12px rgba(150, 170, 255, 0.5);
      border-radius: 50%;
    }
    :host([data-universe='cosmos']) .status-dot {
      box-shadow: 0 0 8px currentColor;
    }
    :host([data-universe='cosmos']) .who-title,
    :host([data-universe='cosmos']) .name {
      color: #c8d4ff;
      text-shadow: 0 0 6px rgba(120, 160, 255, 0.4);
    }
    :host([data-universe='cosmos']) input {
      color: #dfe6ff;
    }
  `;

  override render(): TemplateResult {
    if (!this.state) {
      return html`<div class="connecting">Connecting to the room…</div>`;
    }
    let vm = chatViewModel(this.state);
    // ?demo — inject sample cognition/genome/engine vitals to PREVIEW the rich persona
    // readout before the cognition-emission slice wires the real signals. Explicit flag,
    // never the live default (no fabricated data on a real room).
    if (new URLSearchParams(location.search).has('demo')) {
      vm = {
        ...vm,
        members: vm.members.map((m, i) => {
          // A small spread of real model shapes so the LOADOUT strip previews the
          // formatting (B/T sizes, k/M ctx) — demo-only, never a live default.
          const models = [
            { model: 'devstral-24b', params: 24_000_000_000, contextWindow: 32_768 },
            { model: 'qwen3-coder-30b', params: 30_500_000_000, contextWindow: 262_144 },
            { model: 'claude-opus-4-8', params: 671_000_000_000, contextWindow: 1_000_000 },
          ];
          return {
            ...m,
            vitals: {
              focus: 30 + ((i * 27) % 60),
              reason: 80 - ((i * 19) % 55),
              recall: 45 + ((i * 23) % 45),
              act: 20 + ((i * 31) % 70),
              genome: 33 + ((i * 17) % 60),
              speed: 55 + ((i * 13) % 40),
              size: 40 + ((i * 21) % 45),
            },
            loadout: models[i % models.length],
          };
        }),
      };
    }
    // Digest tier ([[perception-resolution-contract]]): stamp the reader's expand
    // choices onto the rows the projection classified — an expanded row renders
    // its full body with a collapse affordance; everything else stays digested.
    if (this._expanded.size > 0) {
      vm = {
        ...vm,
        messages: vm.messages.map((m) =>
          this._expanded.has(m.id) ? { ...m, expanded: true } : m,
        ),
      };
    }
    // #170 live typing: overlay a transient bubble per persona mid-turn, growing
    // token-by-token, so she visibly types instead of freezing. Resolve each
    // sender's name/kind from the roster (or a prior message) — skip if unknown, to
    // never fabricate an identity. Drop a bubble whose sender just landed the last
    // durable row, so the authoritative message supersedes cleanly.
    if (this._typing.size > 0) {
      const lastSender = vm.messages.at(-1)?.senderId;
      const typingRows: MessageRowVM[] = [];
      for (const [senderId, text] of this._typing) {
        if (senderId === lastSender) continue;
        const member = vm.members.find((m) => m.id === senderId);
        const prior = vm.messages.find((m) => m.senderId === senderId);
        const senderName = member?.name ?? prior?.senderName;
        const kind = member?.kind ?? prior?.kind;
        if (senderName === undefined || kind === undefined) continue;
        typingRows.push({
          id: `typing:${senderId}`,
          senderId,
          senderName,
          kind,
          content: `${text}▋`,
          time: '',
          runtime: member?.runtime ?? prior?.runtime ?? '',
        });
      }
      if (typingRows.length > 0) {
        vm = { ...vm, messages: [...vm.messages, ...typingRows] };
      }
      // The tile's SPEAKING ring: overlay the live token rail onto the roster
      // vitals (`speaking: 100`) for members mid-turn — the same widget-owned
      // overlay pattern as the typing bubbles above and MessageRowVM.expanded.
      // Driven only by REAL StreamDeltas; retires when the turn's `done` lands.
      vm = {
        ...vm,
        members: vm.members.map((m) =>
          this._typing.has(m.id) ? { ...m, vitals: { ...m.vitals, speaking: 100 } } : m,
        ),
      };
    }
    // Error boundary: a render throw (e.g. the Content registry hitting an
    // unregistered room purpose) must be VISIBLE here, not swallowed into a Lit
    // update abort that leaves a silent stuck "Connecting…". Fail loud where it's
    // seen ([[fallbacks-are-illegal-fail-loud]]).
    let surface: TemplateResult;
    try {
      surface = renderChat(vm, { nav: this.nav, sys: this.sys, version: this.version });
    } catch (err) {
      const cause = err instanceof Error ? err.message : String(err);
      return html`<div class="render-error">Interface error rendering this room: ${cause}</div>`;
    }
    const cosmos = this.getAttribute('data-universe') === 'cosmos';
    return html`
      ${cosmos
        ? html`<cosmos-backdrop
            .citizens=${vm.members.map((m) => ({ name: m.name, active: m.active }))}
          ></cosmos-backdrop>`
        : nothing}
      ${surface}
      ${this._selectError ? html`<div class="send-error">${this._selectError}</div>` : nothing}
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
