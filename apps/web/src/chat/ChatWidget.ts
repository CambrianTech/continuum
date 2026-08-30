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
import type { ArenaViewState, CanvasViewState, ChatState } from '@continuum/chat-view';
import {
  chatViewModel,
  focusedLiveTab,
  focusedPersonaTab,
  historyRowsFromPoll,
  type MessageRowVM,
  type RosterMemberVM,
} from '@continuum/chat-view';
import type {
  KanbanViewState,
  NavViewState,
  BenchViewState,
  ServingViewState,
  StreamDelta,
  SystemMetricsViewState,
} from '@continuum/sdk-typescript';
import { renderChat } from './renderChat';
import { CallClient, type CallVideoFrame } from '../live/callClient';
import {
  LISTING_SELECT,
  LIVE_MIC_TOGGLE,
  LIVE_CAPTIONS_TOGGLE,
  LIVE_FACE_TOGGLE,
  SETTINGS_FACE_TOGGLE,
  SETTINGS_AGREE,
  type SettingsFaceToggleDetail,
  type SettingsAgreeDetail,
  MESSAGE_EXPAND_TOGGLE,
  NAV_TAB_CLOSE,
  PANEL_RESIZE_START,
  navSelectTarget,
  type ListingSelectDetail,
  type LiveFaceToggleDetail,
  type MessageExpandToggleDetail,
  type NavTabCloseDetail,
  type PanelResizeStartDetail,
} from '../render/parts';
import { LIVE_PURPOSE, type SettingsContentBody, type WorkspaceLayout } from '@continuum/patterns';
import '../render/CosmosBackdrop'; // registers <cosmos-backdrop> for the cosmos universe

/** The send action the host injects. Resolves when the message is accepted by
 *  the core; rejects (fails loud) on a transport/command error the widget shows. */
export type SendHandler = (text: string) => Promise<void>;

/** The settings action the host injects: fetch (agree undefined) or mutate
 *  (agree set) the node's settings through the SAME core verbs the terminal
 *  uses (`genome/sharing`, `genome/list`) — the widget stays SDK-free and the
 *  face renders substrate truth only, never optimistic local state. */
export type SettingsHandler = (agree?: boolean) => Promise<SettingsContentBody>;

/** The nav-select action the host injects (dispatches `nav/select` through the
 *  command client — the widget stays SDK-free). `kind` is the target's activity
 *  kind: `'room'` switches the room on screen; `'persona'` opens the citizen's
 *  HOME tab (profile/brain) WITHOUT switching the room. Resolves when the core
 *  accepted the select; the VIEW moves only when the refocused chat/nav
 *  envelopes stream back — substrate truth only, no optimistic local state. */
export type SelectRoomHandler = (target: string, kind: 'room' | 'persona') => Promise<void>;

/** The scroll-back fetch the host injects (`chat/poll { beforeMessageId }` —
 *  the Twitter endless-scroll's storage read). Resolves to one page of RAW
 *  stored entities strictly OLDER than the anchor, chronological; an empty
 *  page means history is exhausted. The widget projects and prepends. */
export type HistoryHandler = (
  roomId: string,
  beforeMessageId: string | undefined,
) => Promise<readonly unknown[]>;

/** The tab-close action the host injects (`nav/close`). The tab disappears
 *  when the re-projected nav envelope streams back — never an optimistic
 *  local removal. */
export type CloseTabHandler = (target: string) => Promise<void>;

export class ChatWidget extends LitElement {
  static override properties = {
    state: { attribute: false },
    nav: { attribute: false },
    sys: { attribute: false },
    serving: { attribute: false },
    bench: { attribute: false },
    board: { attribute: false },
    arena: { attribute: false },
    canvas: { attribute: false },
    version: { attribute: false },
    sendHandler: { attribute: false },
    settingsHandler: { attribute: false },
    selectRoomHandler: { attribute: false },
    liveFace: { attribute: false },
    callUrl: { attribute: false },
    _mediaConnected: { state: true },
    _micOn: { state: true },
    _draft: { state: true },
    _sending: { state: true },
    _sendError: { state: true },
    _selectError: { state: true },
    _typing: { state: true },
    _expanded: { state: true },
    _captionsOn: { state: true },
    _history: { state: true },
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

  /** The node's live `kind="serving"` view (the serving glass box, #141),
   *  when the host's subscription has delivered. `undefined` = no widget. */
  serving?: ServingViewState;

  /** The node's live `kind="bench"` benchmark board (#329) — fills the academy
   *  contextual rail with run rows. Honestly absent until the feed delivers. */
  bench?: BenchViewState;

  /** The node's live `kind="kanban"` work board, when the host's subscription
   *  has delivered — feeds the persona home's claims. `undefined` = the claims
   *  section renders its honest awaiting frame. */
  board?: KanbanViewState;

  /** The node's live `kind="arena"` eval-ledger view, when the host's
   *  subscription has delivered — feeds an arena-purpose room's leaderboards.
   *  `undefined` = the arena face renders its honest awaiting frame. */
  arena?: ArenaViewState;

  /** The room's live `kind="canvas"` design-bench observation, when the
   *  host's subscription has delivered — feeds a canvas-purpose run room's
   *  live artifact stage (DESIGN-BENCH-VISUAL-CRAFT.md §5). `undefined` =
   *  the canvas face renders its honest awaiting frame. */
  canvas?: CanvasViewState;

  /** The client build's version string (a real manifest/build stamp injected by
   *  the host) — drives the continuon header's version badge. `undefined` = no
   *  badge, honest. */
  version?: string;

  /** Injected by the host — how a composed message reaches the core. */
  sendHandler?: SendHandler;

  /** Host-injected settings fetch/mutate (see [`SettingsHandler`]). */
  settingsHandler?: SettingsHandler;

  /** Injected by the host — how a rooms-rail pick reaches the core (`nav/select`). */
  selectRoomHandler?: SelectRoomHandler;

  /** Injected by the host — how scroll-back reaches durable storage
   *  (`chat/poll { beforeMessageId }`). Absent = the transcript honestly shows
   *  only the live window (no dead scroll affordance). */
  historyHandler?: HistoryHandler;

  /** Injected by the host — how a tab's × reaches the core (`nav/close`). */
  closeTabHandler?: CloseTabHandler;

  /** A tab's × bubbled up — dispatch the close; failure surfaces in the same
   *  strip as nav failures ([[fallbacks-are-illegal-fail-loud]]). */
  private onNavTabClose = (e: Event): void => {
    const { target } = (e as CustomEvent<NavTabCloseDetail>).detail;
    if (!this.closeTabHandler) {
      throw new Error('<chat-widget>: tab close with no closeTabHandler wired — the host must set it.');
    }
    void this.closeTabHandler(target).catch((err: unknown) => {
      this._selectError = `Tab close failed: ${err instanceof Error ? err.message : String(err)}`;
    });
  };

  /** Scrolled-back rows older than the live window, oldest→newest — the
   *  endless-scroll buffer. Widget-owned presentation state: pages prepend
   *  here, and rows that slide OUT of the live 50-row window retire onto its
   *  tail so no gap ever opens between buffer and window. Cleared on room
   *  switch. Reassigned (not mutated) so Lit re-renders. */
  private _history: MessageRowVM[] = [];
  /** Per-ACTIVITY session state, keyed by room UUID (tab = content = room =
   *  activity — Joel's law). Typing streams and the history buffer belong to
   *  the activity they happened in; switching tabs swaps the pointer, never
   *  clobbers. A stream running in a background room accumulates in ITS
   *  session and is intact when its tab regains focus. */
  private _sessions = new Map<
    string,
    { typing: Map<string, string>; history: MessageRowVM[]; historyExhausted: boolean }
  >();
  /** An empty page came back — the room's history is fully on screen. */
  private _historyExhausted = false;
  /** One in-flight page at a time (the scroll handler fires per frame). */
  private _historyLoading = false;
  /** The `.what` element the scroll listener is attached to (re-attached if
   *  Lit ever swaps the element identity). */
  private _scrollHost?: Element;
  /** The previous render's projected rows — what the window-retirement diff
   *  in `willUpdate` compares against ([[MessageRowVM]] shape, typing rows
   *  excluded at retirement time). */
  private _lastVmMessages: readonly MessageRowVM[] = [];
  /** READER INTENT, not position (the position heuristic broke: a tall
   *  incoming message grows the bottom-distance past any threshold and
   *  auto-scroll silently dies). True only when the USER deliberately
   *  scrolled up; cleared the moment they return to the bottom. Programmatic
   *  scrolls never set it (`_autoScrolling` guards them out). */
  private _userScrolledUp = false;
  /** Set around programmatic scrolls so their scroll events don't read as
   *  user intent. */
  private _autoScrolling = false;

  /** The room's LIVE face is open (the call grid instead of the transcript).
   *  Renderer state, toggled by the header's Go-live affordance / the call
   *  bar's hang-up (composed LIVE_FACE_TOGGLE) — the recipe-declared live
   *  room (purpose "live") is the substrate-driven follow-up. Public so a
   *  host/preview can open the face directly. */
  liveFace = false;

  /** ws:// URL of the core's call server (config: CONTINUUM_CALL_WS, default
   *  8790). Absent = no media plane; the live face stays avatar-presence with
   *  the mic honestly disabled. */
  callUrl?: string;

  private _call?: CallClient;
  private _mediaConnected = false;
  private _micOn = false;
  /** Call-server avatar states: personaId → speaking (merged into the streams
   *  map so the SAME projection drives borders whether speech is tokens on the
   *  chat rail or real audio on the call). */
  private _callSpeaking = new Map<string, boolean>();
  /** senderId → latest decoded video frame, painted onto the tile canvas in
   *  updated() (imperative — canvas is not declarative Lit content). */
  private _videoFrames = new Map<string, CallVideoFrame>();

  private _draft = '';
  private _sending = false;
  private _sendError = '';
  private _selectError = '';
  /** Element navigation (card 95844639): when a tile ELEMENT (compass, genome
   *  block) routed a persona select with an anchor, remember which persona +
   *  section — `updated()` scrolls there once that persona's home renders.
   *  Presentation state only; the wire select is a plain (target, kind). */
  private _pendingAnchor: { readonly persona: string; readonly anchor: string } | null = null;
  /** #170 live typing: senderId → accumulated in-progress turn text. Ephemeral —
   *  the durable message (via `state`) supersedes it; reassigned (not mutated) so
   *  Lit re-renders. */
  private _typing = new Map<string, string>();
  /** Digest-tier expand state ([[perception-resolution-contract]]): the message
   *  ids the reader expanded to full fidelity. Widget-owned presentation state —
   *  the projection classifies, the reader chooses. Reassigned (not mutated) so
   *  Lit re-renders; toggled by the row's bubbled MESSAGE_EXPAND_TOGGLE event. */
  private _expanded = new Set<string>();
  /** The live face's caption strip toggle (CC) — on by default; the strip only
   *  draws while a real turn streams, so "on" costs nothing in silence. */
  private _captionsOn = true;
  /** THE DIRECTORY (Joel, 2026-08-30): the who-panel shows everyone known —
   *  online active, offline greyed — in EVERY room, never a blank. Folded
   *  from each roster seen this session + seeded from persona/list. */
  private _directory = new Map<string, RosterMemberVM>();
  /** Host-injected seed: the node's residents + the viewer themself. */
  directorySeed: readonly RosterMemberVM[] = [];

  /** The Settings face state + its fetched body (substrate truth; undefined
   *  while a fetch is in flight — the face shows its awaiting frame). */
  private settingsFace = false;
  private _settingsBody?: SettingsContentBody;

  /** Open/close the Settings face; opening fetches fresh substrate truth. */
  private onSettingsFaceToggle = (e: Event): void => {
    this.settingsFace = (e as CustomEvent<SettingsFaceToggleDetail>).detail.open;
    if (this.settingsFace) void this.fetchSettings();
    else this._settingsBody = undefined;
    this.requestUpdate();
  };

  /** Covenant accept/revoke from the face — the SAME verb the terminal uses;
   *  the face re-renders from the refetched truth. */
  private onSettingsAgree = (e: Event): void => {
    void this.fetchSettings((e as CustomEvent<SettingsAgreeDetail>).detail.agree);
  };

  private async fetchSettings(agree?: boolean): Promise<void> {
    if (!this.settingsHandler) return; // no host handler = the face stays awaiting (honest)
    try {
      this._settingsBody = await this.settingsHandler(agree);
    } catch (err) {
      this._settingsBody = {
        loaded: true,
        error: err instanceof Error ? err.message : String(err),
        agreed: false,
        covenantVersion: '',
        covenant: '',
        genes: [],
      };
    }
    this.requestUpdate();
  }

  /** Go-live / hang-up: the composed face-toggle from the header affordance or
   *  the call bar bubbles up here — the widget owns the face state. */
  private onLiveFaceToggle = (e: Event): void => {
    this.liveFace = (e as CustomEvent<LiveFaceToggleDetail>).detail.open;
    if (this.liveFace) void this.connectCall();
    else this.disconnectCall();
  };

  /** Dial the core's call server when the face opens (real media plane). A
   *  dial failure keeps the honest avatar-presence face — never a fake
   *  connected state ([[fallbacks-are-illegal-fail-loud]] display-side). */
  private async connectCall(): Promise<void> {
    if (this._call || !this.callUrl || !this.state) return;
    const client = new CallClient({
      onConnected: () => {
        this._mediaConnected = true;
      },
      onClosed: () => {
        this._mediaConnected = false;
        this._micOn = false;
        this._call = undefined;
        this._callSpeaking = new Map();
      },
      onAvatar: (a) => {
        const next = new Map(this._callSpeaking);
        if (a.speaking) next.set(a.personaId, true);
        else next.delete(a.personaId);
        this._callSpeaking = next;
        this.requestUpdate();
      },
      onDelta: (d) => {
        if (this.state) this.applyStreamDelta({ ...d, roomId: this.state.room_id });
      },
      onVideoFrame: (f) => {
        this._videoFrames.set(f.senderId, f);
        this.requestUpdate();
      },
    });
    this._call = client;
    try {
      await client.connect(
        this.callUrl,
        this.state.room_id,
        'operator-web',
        'Operator (web)',
      );
    } catch {
      this._call = undefined;
      this._mediaConnected = false;
    }
  }

  private disconnectCall(): void {
    this._call?.leave();
    this._call = undefined;
    this._mediaConnected = false;
    this._micOn = false;
    this._callSpeaking = new Map();
    this._videoFrames = new Map();
  }

  /** The mic button — a REAL toggle when the media plane is connected. */
  private onLiveMicToggle = (): void => {
    const call = this._call;
    if (!call) return;
    if (this._micOn) {
      call.stopMic();
      this._micOn = false;
    } else {
      void call.startMic().then((ok) => {
        this._micOn = ok;
      });
    }
  };

  /** CC toggle — flips the live caption strip (a real control). */
  private onLiveCaptionsToggle = (): void => {
    this._captionsOn = !this._captionsOn;
  };

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
    const route = navSelectTarget((e as CustomEvent<ListingSelectDetail>).detail);
    if (route === null) return;
    if (!this.selectRoomHandler) {
      // Fail loud: a selectable rail with no wired switch is a wiring
      // bug, not a no-op ([[fallbacks-are-illegal-fail-loud]]).
      throw new Error(
        '<chat-widget>: nav select with no selectRoomHandler wired — the host must set it.',
      );
    }
    this._selectError = '';
    // An anchored route remembers WHERE in the destination to land; the scroll
    // happens after the persona home renders (updated()), never optimistically.
    this._pendingAnchor =
      route.kind === 'persona' && route.anchor !== undefined
        ? { persona: route.target, anchor: route.anchor }
        : null;
    void this.selectRoomHandler(route.target, route.kind).catch((err: unknown) => {
      // Surface the failure in-UI; never a silently-dead click.
      this._selectError = `Navigation failed: ${err instanceof Error ? err.message : String(err)}`;
    });
  };

  /**
   * Apply one live token from a persona's in-progress turn (#170). Grows a transient
   * "typing" bubble keyed by sender; `done` retires it. Deltas for other rooms are
   * ignored. Never touches `state` — the authoritative message still arrives there.
   */
  applyStreamDelta(delta: StreamDelta): void {
    if (delta.roomId !== this.state?.room_id) {
      // A stream in ANOTHER activity accumulates in THAT activity's session —
      // never in the tab being viewed (the "(Benchy) is responding… in every
      // tab" leak, Joel 2026-08-30). It's all there when its tab regains focus.
      const sess = this._sessions.get(delta.roomId) ?? {
        typing: new Map<string, string>(),
        history: [],
        historyExhausted: false,
      };
      if (delta.done) sess.typing.delete(delta.senderId);
      else sess.typing.set(delta.senderId, (sess.typing.get(delta.senderId) ?? '') + delta.token);
      this._sessions.set(delta.roomId, sess);
      return;
    }
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
    // Tab close: the ×'s composed NAV_TAB_CLOSE bubbles up the same way.
    this.addEventListener(NAV_TAB_CLOSE, this.onNavTabClose);
    // The live face: Go-live/hang-up + the CC toggle bubble up the same way.
    this.addEventListener(LIVE_FACE_TOGGLE, this.onLiveFaceToggle);
    this.addEventListener(SETTINGS_FACE_TOGGLE, this.onSettingsFaceToggle);
    this.addEventListener(SETTINGS_AGREE, this.onSettingsAgree);
    this.addEventListener(LIVE_MIC_TOGGLE, this.onLiveMicToggle);
    this.addEventListener(LIVE_CAPTIONS_TOGGLE, this.onLiveCaptionsToggle);
  }

  override disconnectedCallback(): void {
    this.removeEventListener(MESSAGE_EXPAND_TOGGLE, this.onExpandToggle);
    this.removeEventListener(LISTING_SELECT, this.onListingSelect);
    this.removeEventListener(NAV_TAB_CLOSE, this.onNavTabClose);
    this.removeEventListener(LIVE_FACE_TOGGLE, this.onLiveFaceToggle);
    this.removeEventListener(LIVE_CAPTIONS_TOGGLE, this.onLiveCaptionsToggle);
    super.disconnectedCallback();
  }

  static override styles = css`
    /* Styled ENTIRELY from the shared design tokens (apps/web/src/theme.css) — no
     * hardcoded colors, so a theme swap is a :root override and the same token
     * names port to other surfaces. */
    :host {
      /* The shell is ONE full-height grid (.panels) — Discord geometry: the
         rails run window-top to window-bottom; tabs, header, transcript, and
         compose all live inside the CENTER column. */
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
         the slim tracks between are the drag handles. Columns run FULL HEIGHT
         (Discord/VS Code): no chrome bar spans the whole window. */
      grid-template-columns: var(--who-w, 280px) 6px 1fr;
      min-height: 0;
      flex: 1;
    }
    /* A populated ContextPanel opens the third column — the right contextual
       rail (participants summary, room info; the factory reference's right panel). */
    .panels[data-context] {
      grid-template-columns: var(--who-w, 280px) 6px 1fr 6px var(--ctx-w, 220px);
    }
    /* The CENTER column — tabs on top (VS Code-central), then the room header,
       then the transcript taking the remainder, then the host-supplied compose
       footer. All center-scoped; the rails never share a row with any of it. */
    .center {
      display: flex;
      flex-direction: column;
      min-width: 0;
      min-height: 0;
    }
    .center > .what {
      flex: 1;
      min-height: 0;
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
      font-size: 14px;
      line-height: 1;
      cursor: pointer;
      /* A REAL hit target (Joel: "super small hitbox") — ~22px square via
         padding + negative margin so the visual glyph stays compact while the
         clickable area meets the finger/pointer minimum. */
      padding: 6px 7px;
      margin: -6px -5px -6px -3px;
      border-radius: var(--radius-sm);
    }
    .tab-close:hover {
      color: var(--content-primary);
      background: var(--button-secondary-background);
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
    /* The system HUD keeps ONE height across its faces — a rail that reflows
     * every time a face changes reads as broken chrome (Joel, 2026-08-30:
     * "dynamically change heights all the time"). Reserve the tallest face. */
    .rail-widget[data-widget='system'] {
      min-height: 128px;
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
    /* SERVING glass box (#141 slice 1) — header line, bandit arm chips with
     * reward bars, and the pager's event cards. Sparklines reuse .gauge. */
    .serving-line {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
      padding: 0 var(--spacing-md) var(--spacing-xs);
      font-size: 11px;
      min-width: 0;
    }
    .serving-model {
      font-family: var(--font-mono);
      font-weight: 700;
      color: var(--content-primary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      min-width: 0;
    }
    .serving-meta {
      flex-shrink: 0;
      font-size: 9.5px;
      letter-spacing: 0.05em;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    .serving-degraded {
      color: var(--status-warning, #e0a458);
    }
    .serving-arms {
      display: flex;
      gap: 4px;
      padding: var(--spacing-xs) var(--spacing-md) 0;
    }
    .serving-arm {
      position: relative;
      flex: 1;
      min-width: 0;
      padding: 2px 0 4px;
      text-align: center;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      overflow: hidden;
    }
    /* BENCH board (#329) — console-grade: scoreboard header, state-dot
     * cards with a real acts progress bar, patch-forming accent chip, and
     * the REGRESSION alarm. Working dots pulse (reduced-motion: static). */
    .bench-board {
      display: flex;
      flex-direction: column;
      gap: 8px;
      padding: var(--spacing-xs) var(--spacing-md) var(--spacing-sm);
    }
    .bench-score {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 6px;
      margin-bottom: 2px;
    }
    /* In-flight ROUND rows (#371) — the tracker's lifecycle truth above the
     * per-run cards: suite name, stage, settled/dispatched with a settle bar,
     * and the citizen/detached driver tag (detached = no curriculum, dimmed). */
    .bench-rounds {
      display: flex;
      flex-direction: column;
      gap: 4px;
    }
    .bench-round {
      position: relative;
      display: grid;
      grid-template-columns: auto auto auto 1fr auto;
      align-items: center;
      gap: 8px;
      padding: 6px 10px 6px 12px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: color-mix(in srgb, var(--surface, #0b1220) 65%, transparent);
      font-size: 11px;
      overflow: hidden;
    }
    /* Stage stripe: the round's lifecycle as a left edge of light —
     * working breathes in the accent, done settles into success. */
    .bench-round::before {
      content: '';
      position: absolute;
      inset: 0 auto 0 0;
      width: 3px;
      background: var(--accent-primary);
      box-shadow: 0 0 8px color-mix(in srgb, var(--accent-primary) 60%, transparent);
      animation: bench-round-breathe 2.4s ease-in-out infinite;
    }
    .bench-round[data-stage='done']::before {
      background: var(--status-success, #4caf7d);
      box-shadow: 0 0 6px color-mix(in srgb, var(--status-success, #4caf7d) 50%, transparent);
      animation: none;
    }
    @keyframes bench-round-breathe {
      0%, 100% { opacity: 1; }
      50% { opacity: 0.45; }
    }
    .bench-round-name {
      font-weight: 700;
      letter-spacing: 0.02em;
      white-space: nowrap;
      color: var(--content-primary);
    }
    .bench-round-stage {
      text-transform: uppercase;
      font-size: 8.5px;
      letter-spacing: 0.12em;
      padding: 1px 6px;
      border-radius: 999px;
      border: 1px solid color-mix(in srgb, var(--accent-primary) 40%, transparent);
      color: var(--accent-primary);
      background: color-mix(in srgb, var(--accent-primary) 10%, transparent);
    }
    .bench-round[data-stage='done'] .bench-round-stage {
      border-color: color-mix(in srgb, var(--status-success, #4caf7d) 40%, transparent);
      color: var(--status-success, #4caf7d);
      background: color-mix(in srgb, var(--status-success, #4caf7d) 10%, transparent);
    }
    .bench-round-count {
      font-variant-numeric: tabular-nums;
      font-weight: 700;
      white-space: nowrap;
      color: var(--content-primary);
    }
    .bench-round .bench-bar {
      margin: 0;
    }
    /* Settle bar earns a gradient + glow — progress is the row's headline. */
    .bench-round .bench-bar-fill {
      background: linear-gradient(
        90deg,
        color-mix(in srgb, var(--accent-primary) 75%, transparent),
        var(--status-success, #4caf7d)
      );
      box-shadow: 0 0 6px color-mix(in srgb, var(--status-success, #4caf7d) 45%, transparent);
    }
    .bench-round-driver {
      font-size: 8.5px;
      text-transform: uppercase;
      letter-spacing: 0.1em;
      padding: 1px 6px;
      border-radius: 3px;
      border: 1px solid var(--border-subtle);
      color: var(--content-secondary);
    }
    .bench-round-detached {
      opacity: 0.45;
    }
    @media (prefers-reduced-motion: reduce) {
      .bench-round::before {
        animation: none;
      }
    }
    .bench-stat {
      display: flex;
      flex-direction: column;
      align-items: center;
      padding: 8px 4px 6px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: color-mix(in srgb, var(--surface, #0b1220) 60%, transparent);
    }
    .bench-stat-n {
      font-size: 22px;
      font-weight: 800;
      line-height: 1;
      font-variant-numeric: tabular-nums;
      color: var(--content-primary);
    }
    .bench-stat-l {
      margin-top: 3px;
      font-size: 8.5px;
      letter-spacing: 0.12em;
      text-transform: uppercase;
      color: var(--content-secondary);
    }
    .bench-stat-resolved .bench-stat-n { color: var(--status-success, #4caf7d); }
    .bench-stat-working .bench-stat-n { color: var(--accent-primary); }
    .bench-stat-failed .bench-stat-n { color: var(--content-secondary); }
    .bench-stall-banner {
      padding: 5px 8px;
      border: 1px solid color-mix(in srgb, var(--status-warning, #e0a458) 45%, transparent);
      border-radius: var(--radius-sm);
      background: color-mix(in srgb, var(--status-warning, #e0a458) 12%, transparent);
      color: var(--status-warning, #e0a458);
      font-size: 9.5px;
      font-weight: 600;
    }
    .bench-card {
      display: flex;
      flex-direction: column;
      gap: 5px;
      padding: 8px 10px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: color-mix(in srgb, var(--surface, #0b1220) 45%, transparent);
      font-size: 10px;
      color: var(--content-secondary);
    }
    .bench-state-queued { opacity: 0.65; }
    .bench-card-head {
      display: flex;
      align-items: center;
      gap: 7px;
      min-width: 0;
    }
    .bench-dot {
      flex-shrink: 0;
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--border-subtle);
    }
    .bench-state-working .bench-dot,
    .bench-state-grading .bench-dot {
      background: var(--accent-primary);
      box-shadow: 0 0 6px var(--accent-primary);
      animation: bench-pulse 1.6s ease-in-out infinite;
    }
    .bench-state-stalled .bench-dot { background: var(--status-warning, #e0a458); }
    .bench-state-resolved .bench-dot { background: var(--status-success, #4caf7d); }
    .bench-state-failed .bench-dot { background: var(--status-error, #d9534f); }
    @keyframes bench-pulse {
      0%, 100% { opacity: 1; transform: scale(1); }
      50% { opacity: 0.45; transform: scale(0.8); }
    }
    @media (prefers-reduced-motion: reduce) {
      .bench-dot { animation: none !important; }
    }
    .bench-instance {
      flex: 1;
      min-width: 0;
      font-family: var(--font-mono);
      font-weight: 700;
      font-size: 11px;
      color: var(--content-primary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .bench-attempt {
      flex-shrink: 0;
      font-size: 10px;
      font-weight: 700;
      font-variant-numeric: tabular-nums;
      color: var(--content-secondary);
    }
    .bench-attempt i { font-style: normal; opacity: 0.5; padding: 0 1px; }
    .bench-card-meta {
      display: flex;
      align-items: baseline;
      flex-wrap: wrap;
      gap: 8px;
      font-size: 9.5px;
    }
    .bench-persona { font-weight: 700; color: var(--content-primary); }
    .bench-selfclaimed {
      font-size: 8px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--accent-primary);
    }
    .bench-pulse { font-variant-numeric: tabular-nums; }
    .bench-nogen { font-style: italic; opacity: 0.7; }
    .bench-patch {
      margin-left: auto;
      font-family: var(--font-mono);
      font-weight: 800;
      font-size: 9.5px;
      color: var(--accent-primary);
    }
    .bench-bar {
      height: 3px;
      border-radius: 2px;
      background: color-mix(in srgb, var(--border-subtle) 60%, transparent);
      overflow: hidden;
    }
    .bench-bar-fill {
      height: 100%;
      border-radius: 2px;
      background: linear-gradient(90deg,
        color-mix(in srgb, var(--accent-primary) 45%, transparent),
        var(--accent-primary));
    }
    .bench-state-resolved .bench-bar-fill { background: var(--status-success, #4caf7d); }
    .bench-state-stalled .bench-bar-fill { background: var(--status-warning, #e0a458); }
    .bench-verdict {
      display: flex;
      flex-wrap: wrap;
      align-items: baseline;
      gap: 7px;
      padding-top: 1px;
      font-size: 9.5px;
    }
    .bench-verdict b { color: var(--content-primary); font-variant-numeric: tabular-nums; }
    .bench-resolved {
      font-weight: 800;
      letter-spacing: 0.06em;
      color: var(--status-success, #4caf7d);
    }
    .bench-alarm {
      font-weight: 800;
      letter-spacing: 0.04em;
      color: var(--status-error, #d9534f);
    }
    .bench-failed {
      font-family: var(--font-mono);
      font-size: 8.5px;
      opacity: 0.8;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      max-width: 100%;
    }
    .bench-lanes,
    .bench-snapshot-banner,
    .bench-awaiting {
      font-size: 9.5px;
      font-style: italic;
      color: var(--content-secondary);
    }
    .serving-arm[data-chosen] {
      border-color: var(--accent-primary);
    }
    .serving-arm[data-chosen] .arm-label {
      color: var(--accent-primary);
      font-weight: 700;
    }
    .arm-label {
      font-family: var(--font-mono);
      font-size: 8.5px;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    .arm-bar {
      position: absolute;
      left: 0;
      bottom: 0;
      height: 2px;
      background: var(--accent-primary);
      opacity: 0.85;
      border-radius: 1px;
    }
    ul.serving-events {
      list-style: none;
      margin: 0;
      padding: var(--spacing-xs) var(--spacing-md) var(--spacing-sm);
      display: flex;
      flex-direction: column;
      gap: 3px;
    }
    .serving-event {
      display: flex;
      align-items: baseline;
      gap: 6px;
      font-size: 9.5px;
      padding: 2px 6px;
      border-left: 2px solid var(--border-subtle);
      background: var(--widget-surface, rgba(255, 255, 255, 0.03));
      border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
      color: var(--content-secondary);
    }
    .serving-event[data-kind='decay-switch'] {
      border-left-color: var(--accent-primary);
    }
    .serving-event[data-kind='serve-start'] {
      border-left-color: var(--status-success, #4caf7d);
    }
    .event-token {
      flex-shrink: 0;
      font-family: var(--font-mono);
      font-size: 8.5px;
      color: var(--content-tertiary, var(--content-secondary));
      font-variant-numeric: tabular-nums;
    }
    .event-detail {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    /* HUD cycle/pin toggle — the far-left corner control of the one graph
     * control: ⟳ while auto-cycling faces, ◉ when pinned. */
    .hud-toggle {
      appearance: none;
      border: 1px solid var(--border-subtle);
      background: transparent;
      color: var(--content-secondary);
      border-radius: 50%;
      width: 16px;
      height: 16px;
      font-size: 9px;
      line-height: 1;
      padding: 0;
      cursor: pointer;
      flex-shrink: 0;
      display: inline-flex;
      align-items: center;
      justify-content: center;
    }
    .hud-toggle[data-cycling] {
      color: var(--accent-primary);
      border-color: var(--accent-primary);
    }
    .hud-toggle:hover {
      color: var(--content-primary);
    }
    /* SERVING CONSOLE (purpose="serving") — the machine room center-stage:
     * per-node panels, headline tok/s numeral, full-width instrument, arm
     * bank, control-loop feed. Console legibility: big numerals, wide
     * instruments, generous rhythm. */
    .srv-console {
      display: flex;
      flex-direction: column;
      gap: var(--spacing-lg);
      padding: var(--spacing-lg);
      overflow-y: auto;
      height: 100%;
    }
    .srv-snapshot {
      align-self: flex-start;
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      padding: 2px 8px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      color: var(--content-secondary);
    }
    .srv-node {
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-md, 8px);
      background: var(--widget-surface, rgba(255, 255, 255, 0.02));
      padding: var(--spacing-md) var(--spacing-lg) var(--spacing-lg);
      display: flex;
      flex-direction: column;
      gap: var(--spacing-md);
    }
    .srv-banner {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-md);
      min-width: 0;
    }
    .srv-node-name {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      font-size: 11px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--content-secondary);
      flex-shrink: 0;
    }
    .srv-node-name[data-local] {
      color: var(--content-primary);
    }
    .srv-local-chip {
      font-size: 8px;
      letter-spacing: 0.1em;
      padding: 1px 5px;
      border-radius: var(--radius-sm);
      background: var(--accent-primary);
      color: var(--surface, #0a0e14);
      text-transform: uppercase;
    }
    .srv-model {
      font-family: var(--font-mono);
      font-size: 16px;
      font-weight: 700;
      color: var(--content-primary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      min-width: 0;
    }
    .srv-pulse {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--status-warning, #e0a458);
      flex-shrink: 0;
      align-self: center;
    }
    .srv-pulse[data-ready='true'] {
      background: var(--status-success, #4caf7d);
      box-shadow: 0 0 6px var(--status-success, #4caf7d);
    }
    .srv-lanes {
      font-size: 11px;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
      flex-shrink: 0;
    }
    .srv-degraded {
      font-size: 11px;
      color: var(--status-warning, #e0a458);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .srv-headline {
      margin-left: auto;
      display: inline-flex;
      align-items: baseline;
      gap: 6px;
      flex-shrink: 0;
    }
    .srv-headline-num {
      font-family: var(--font-mono);
      font-size: 34px;
      font-weight: 700;
      line-height: 1;
      color: var(--accent-primary);
      font-variant-numeric: tabular-nums;
    }
    .srv-headline-unit {
      font-size: 10px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--content-secondary);
    }
    .srv-instrument .gauge {
      padding: 0;
    }
    .srv-instrument .gauge svg {
      height: 120px;
    }
    .srv-section-label {
      display: block;
      font-size: 9px;
      letter-spacing: 0.12em;
      text-transform: uppercase;
      color: var(--content-secondary);
      margin-bottom: 4px;
    }
    .srv-bank-arms {
      padding: 0;
      gap: 6px;
    }
    .srv-bank .serving-arm {
      padding: 5px 0 7px;
    }
    .srv-bank .arm-label {
      font-size: 11px;
    }
    .arm-reward {
      display: block;
      font-family: var(--font-mono);
      font-size: 9px;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    .srv-bank .arm-bar {
      height: 3px;
    }
    .srv-feed-events {
      padding: 0;
      gap: 4px;
    }
    .srv-feed .serving-event {
      font-size: 11px;
      padding: 4px 8px;
    }
    .srv-awaiting {
      margin: auto;
      text-align: center;
      color: var(--content-secondary);
    }
    .srv-awaiting-title {
      font-size: 14px;
      letter-spacing: 0.1em;
      text-transform: uppercase;
      margin-bottom: 6px;
    }
    .srv-awaiting-line {
      font-size: 11px;
    }
    /* GRID view — the node panel's sections (resources / serving) and the
     * node name sized as the panel's identity when no model banner leads. */
    .grid-node-name {
      font-size: 14px;
      letter-spacing: 0.1em;
    }
    .grid-section {
      display: flex;
      flex-direction: column;
      gap: 4px;
    }
    .grid-section .gauge {
      padding: 0;
    }
    .grid-section .gauge svg {
      height: 88px;
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
      /* A designed grid, not accidental flex alignment: avatar | info | right
         rail, with the recency stamp and genome panel stacked in the right rail
         (no absolute positioning, no dodge margins — the reference layout). */
      display: grid;
      grid-template-columns: auto 1fr auto;
      grid-template-areas:
        'av info ago'
        'av info gen';
      grid-template-rows: auto 1fr;
      column-gap: var(--spacing-sm);
      row-gap: 2px;
      align-items: center;
      padding: var(--spacing-xs) var(--spacing-sm);
      /* Chamfered HUD module (notched corners), not a rounded row. */
      clip-path: polygon(0 0, calc(100% - 7px) 0, 100% 7px, 100% 100%, 7px 100%, 0 calc(100% - 7px));
      transition: background var(--motion-fast) var(--motion-ease);
    }
    .member .avatar {
      grid-area: av;
    }
    .member .info {
      grid-area: info;
    }
    /* Dormant mind: vitals wired, every cognition pulse dark, no live stream —
       the row recedes slightly; any pulse/afterglow/stream restores full
       brightness (transition makes waking visible). Opacity only: compositor-
       cheap, theme-neutral (the universe layer can restyle later, #260). */
    .member.dormant {
      opacity: 0.62;
      transition: opacity 0.9s ease;
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
      gap: 1px;
      min-width: 0;
    }
    /* Identity line — kind/runtime chips and the model loadout share ONE row
       (they were two stacked rows; the tile's height budget goes to live data). */
    .member .idline {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
      min-width: 0;
    }
    .member .idline .loadout {
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
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
      grid-area: ago;
      justify-self: end;
      align-self: start;
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
      /* 2×2 grid — four live bars in two rows, half the vertical spend of the
         stacked column, no data dropped. */
      display: grid;
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 2px var(--spacing-sm);
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
      grid-area: gen;
      justify-self: end;
      align-self: start;
      display: grid;
      grid-template-columns: auto 1fr auto;
      column-gap: var(--spacing-xs);
      align-items: end;
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
      /* Two rows of four (8 slots) — the loadout grid, row-major so genes
         fill left-to-right, top-to-bottom as they page in. */
      display: grid;
      grid-template-columns: repeat(4, auto);
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
    /* Element navigation (card 95844639): a tile element that navigates SIGNALS
     * it — pointer + a lift on hover (the affordance IS the invitation). */
    .element-link {
      cursor: pointer;
      transition: filter var(--motion-base) var(--motion-ease),
        transform var(--motion-base) var(--motion-ease);
    }
    .element-link:hover {
      filter: brightness(1.35) drop-shadow(0 0 4px var(--hud-accent-glow));
      transform: translateY(-1px);
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
    /* Tool-act receipt rows (#243) — the collapsed "Read 2 files, ran a command ›"
       line between speech, expanding IN PLACE via native <details>. Quiet by
       design: the work is one gesture away, never shouting over the words. */
    .messages .act-group {
      list-style: none;
      padding: 1px 0;
      margin-left: calc(28px + var(--spacing-sm));
      font-size: 11px;
      color: var(--content-secondary);
    }
    .act-group summary {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      cursor: pointer;
      user-select: none;
      padding: 2px 8px;
      border-radius: var(--radius-sm);
      border: 1px solid var(--border-subtle);
      background: color-mix(in srgb, var(--surface, #0b1220) 40%, transparent);
    }
    .act-group summary::-webkit-details-marker { display: none; }
    .act-group summary::after { content: '›'; opacity: 0.6; transition: transform 0.15s; }
    .act-group details[open] summary::after { transform: rotate(90deg); }
    .act-gear { opacity: 0.75; }
    .act-gear.act-failed { color: var(--status-warning, #e0a458); }
    .act-actor { font-weight: 600; color: var(--content-primary); }
    .act-count {
      font-size: 9px;
      padding: 0 5px;
      border-radius: 8px;
      background: color-mix(in srgb, var(--content-secondary) 18%, transparent);
      font-variant-numeric: tabular-nums;
    }
    .act-list {
      list-style: none;
      margin: 4px 0 2px;
      padding: 0 0 0 14px;
      border-left: 1px solid var(--border-subtle);
    }
    .act-item {
      display: flex;
      align-items: baseline;
      gap: 7px;
      padding: 1.5px 0;
      font-family: var(--font-mono, ui-monospace, monospace);
      font-size: 10.5px;
    }
    .act-item .act-mark { color: var(--status-success, #4caf7d); }
    .act-item.act-failed .act-mark { color: var(--status-error, #e05858); }
    .act-tool { color: var(--accent-primary); }
    .act-obj {
      color: var(--content-secondary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      max-width: 420px;
    }
    .act-time { margin-left: auto; opacity: 0.5; font-size: 9px; }
    /* Continuation rows (same sender, grouped upstream): tuck the body into the
       sender's column — tight runs, the classic chat grouping. */
    .messages .msg.continues {
      padding: 1px 0;
      margin-left: calc(28px + var(--spacing-sm));
    }
    .msg-glyph {
      flex: none;
      width: 28px;
      height: 28px;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .msg-avatar {
      width: 28px;
      height: 28px;
      border-radius: 50%;
      object-fit: cover;
      object-position: center top;
      border: 1px solid var(--hud-accent-border);
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
    /* Line-numbered body: gutter + code side by side, one shared line-height so
       numbers stay glued to their lines; only the code pane scrolls horizontally. */
    .code-body {
      display: flex;
      align-items: flex-start;
    }
    .code-gutter {
      flex: none;
      padding: 2px 0 8px 11px;
      text-align: right;
      min-width: 2.5ch;
      user-select: none;
      white-space: pre;
      font-family: var(--font-mono);
      font-size: 12.5px;
      line-height: 1.5;
      color: var(--content-secondary);
      opacity: 0.5;
    }
    .code-collapsible pre {
      flex: 1;
      min-width: 0;
      margin: 0;
      padding: 2px 11px 8px 10px;
      overflow-x: auto;
      font-family: var(--font-mono);
      font-size: 12.5px;
      line-height: 1.5;
      color: var(--content-accent);
    }
    .code-collapsible pre code {
      white-space: pre;
    }
    /* "+K more lines" expander for big blocks — the head stays visible, the rest
       is one click away; expanded, the gutter numbering continues seamlessly. */
    .code-more summary {
      cursor: pointer;
      padding: 4px 11px 6px;
      font-family: var(--font-mono);
      font-size: 11px;
      color: var(--content-secondary);
      list-style: none;
      user-select: none;
      border-top: 1px solid var(--border-subtle);
    }
    .code-more summary:hover {
      color: var(--content-accent);
    }
    .code-more summary::-webkit-details-marker {
      display: none;
    }
    .code-more[open] summary {
      display: none;
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
    /* "(xyz, abc) is responding…" — ONE grey line pinned between the last
       message and the compose box; the name list updates as turns start/settle.
       Never wraps, never grows: overflow elides. */
    .responding-line {
      padding: 3px 14px 4px;
      font-size: 11.5px;
      color: var(--content-secondary);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
      opacity: 0.85;
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

    /* ================= PERSONA HOME =================
     * The persona's home activity (purpose "persona") — profile hero + the
     * Cognitive System View, styled to the reference HUD (docs/images/
     * persona-profile.png + persona-brain-hud.png). Every colour is a named
     * token; region glow rides the live faculty pulse via --region-level. */
    /* ── ARENA face (purpose "arena") — leaderboards from the real ledger ── */
    .arena-home {
      display: flex;
      flex-direction: column;
      gap: 14px;
      padding: 16px 18px;
      overflow-y: auto;
    }
    .arena-head {
      display: flex;
      align-items: center;
      gap: 10px;
    }
    .arena-title {
      font-size: 15px;
      font-weight: 800;
      letter-spacing: 3px;
      color: var(--hud-accent);
    }
    .a-feed-chip {
      font-size: 9px;
      letter-spacing: 1px;
      text-transform: uppercase;
      padding: 2px 7px;
      border-radius: 8px;
      border: 1px solid var(--panel-border);
      color: var(--text-dim);
    }
    .a-feed-chip[data-on] {
      color: var(--status-online);
      border-color: var(--status-online);
    }
    .a-rowcount {
      margin-left: auto;
      font-size: 10px;
      color: var(--text-dim);
    }
    .arena-live-run {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 8px 12px;
      border: 1px solid var(--hud-accent);
      border-radius: 8px;
      background: var(--hud-panel-background);
      font-size: 11px;
    }
    .a-live-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      background: var(--status-online);
      box-shadow: 0 0 6px var(--status-online);
      animation: pulse-dot 1.2s ease-in-out infinite;
    }
    @keyframes pulse-dot {
      50% { opacity: 0.35; }
    }
    .a-live-label {
      font-weight: 800;
      letter-spacing: 2px;
      font-size: 9px;
      color: var(--status-online);
    }
    .a-live-what { font-weight: 600; }
    .a-live-progress {
      flex: 1;
      height: 5px;
      border-radius: 3px;
      background: var(--hud-slot-background);
      overflow: hidden;
    }
    .a-live-fill {
      display: block;
      height: 100%;
      background: var(--hud-accent);
      transition: width var(--motion-slow) var(--motion-ease);
    }
    .a-live-count { font-variant-numeric: tabular-nums; color: var(--text-dim); }
    .a-live-task { color: var(--text-dim); font-style: italic; }
    .arena-board {
      border: 1px solid var(--panel-border);
      border-radius: 10px;
      background: var(--hud-panel-background);
      overflow: hidden;
    }
    .a-board-head {
      display: flex;
      align-items: baseline;
      gap: 8px;
      padding: 8px 12px;
      border-bottom: 1px solid var(--panel-border);
    }
    .a-board-name {
      font-weight: 700;
      font-size: 12px;
      letter-spacing: 1px;
      color: var(--hud-accent);
    }
    .a-board-count {
      margin-left: auto;
      font-size: 10px;
      color: var(--text-dim);
    }
    .arena-table {
      width: 100%;
      border-collapse: collapse;
      font-size: 11px;
    }
    .arena-table th {
      text-align: left;
      font-size: 9px;
      letter-spacing: 1px;
      text-transform: uppercase;
      color: var(--text-dim);
      padding: 5px 10px;
      border-bottom: 1px solid var(--panel-border);
    }
    .arena-table td {
      padding: 6px 10px;
      border-bottom: 1px solid var(--panel-border);
      vertical-align: middle;
    }
    .arena-row:last-child td { border-bottom: none; }
    .arena-row[data-excluded] {
      opacity: 0.5;
    }
    .arena-row[data-excluded] .a-model-name,
    .arena-row[data-excluded] .a-score-num {
      text-decoration: line-through;
    }
    .a-rank {
      width: 26px;
      font-weight: 800;
      color: var(--hud-accent);
      font-variant-numeric: tabular-nums;
    }
    .a-model-name { font-weight: 600; }
    .a-arm {
      margin-left: 6px;
      font-size: 8px;
      letter-spacing: 1px;
      padding: 1px 5px;
      border-radius: 6px;
      border: 1px solid var(--panel-border);
      color: var(--text-dim);
      text-transform: uppercase;
    }
    .a-arm[data-arm='OURS'] {
      color: var(--hud-accent);
      border-color: var(--hud-accent);
    }
    .a-score { display: flex; align-items: center; gap: 8px; min-width: 180px; }
    .a-score-bar {
      flex: 1;
      height: 6px;
      border-radius: 3px;
      background: var(--hud-slot-background);
      overflow: hidden;
    }
    .a-score-fill {
      display: block;
      height: 100%;
      background: var(--hud-accent);
      box-shadow: 0 0 4px var(--hud-accent-glow);
    }
    .a-score-num { font-variant-numeric: tabular-nums; font-weight: 700; }
    .a-score-pct { font-variant-numeric: tabular-nums; color: var(--text-dim); font-size: 10px; }
    .a-prov, .a-date { color: var(--text-dim); font-size: 10px; white-space: nowrap; }
    .a-awaiting {
      padding: 22px;
      text-align: center;
      color: var(--text-dim);
      border: 1px dashed var(--panel-border);
      border-radius: 10px;
    }

    .persona-home {
      max-width: 880px;
      margin: 0 auto;
      display: flex;
      flex-direction: column;
      gap: var(--spacing-lg);
      padding-bottom: var(--spacing-xl);
    }
    .p-awaiting-banner {
      padding: var(--spacing-sm) var(--spacing-md);
      border: 1px dashed var(--border-subtle);
      border-radius: var(--radius-md);
      color: var(--content-secondary);
      font-size: 12px;
      font-style: italic;
    }
    /* --- hero ---------------------------------------------------------- */
    .p-hero {
      display: flex;
      align-items: center;
      gap: var(--spacing-xl);
      padding: var(--spacing-xl) var(--spacing-md) var(--spacing-md);
    }
    .p-avatar {
      position: relative;
      width: 148px;
      height: 148px;
      border-radius: 50%;
      flex: none;
      display: grid;
      place-items: center;
      font-size: 64px;
      background: radial-gradient(circle at 50% 35%, rgba(0, 212, 255, 0.12), transparent 70%),
        var(--hud-panel-background);
      border: 3px solid var(--hud-accent-border);
      box-shadow: 0 0 24px var(--hud-accent-glow), inset 0 0 18px rgba(0, 0, 0, 0.5);
    }
    .p-avatar[data-online] {
      animation: alive-pulse 3.2s var(--motion-ease) infinite;
    }
    .p-avatar-img {
      position: absolute;
      inset: 0;
      width: 100%;
      height: 100%;
      border-radius: 50%;
      object-fit: cover;
      object-position: center top;
    }
    .p-presence-dot {
      position: absolute;
      bottom: 8px;
      right: 8px;
      width: 18px;
      height: 18px;
      border-radius: 50%;
      background: var(--status-offline);
      border: 3px solid var(--widget-surface-solid);
      z-index: 2;
    }
    .p-avatar[data-online] .p-presence-dot {
      background: var(--status-online);
      box-shadow: 0 0 8px var(--status-online);
    }
    .p-id {
      min-width: 0;
      display: flex;
      flex-direction: column;
      gap: var(--spacing-sm);
    }
    .p-name {
      margin: 0;
      font-size: 30px;
      font-weight: 700;
      letter-spacing: 0.01em;
      color: var(--content-primary);
      text-shadow: 0 0 14px var(--hud-accent-glow);
    }
    .p-handle {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
      color: var(--content-secondary);
      font-size: 13px;
    }
    .p-dot {
      opacity: 0.5;
    }
    .p-online[data-on] {
      color: var(--status-online);
    }
    .p-chips {
      display: flex;
      flex-wrap: wrap;
      gap: var(--spacing-sm);
      align-items: center;
    }
    .p-chip {
      padding: 2px 10px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-lg);
      font-family: var(--font-mono);
      font-size: 10px;
      font-weight: 700;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--content-secondary);
      background: var(--widget-surface, rgba(255, 255, 255, 0.03));
    }
    .p-chip-model {
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
      color: var(--content-accent);
      text-transform: none;
    }
    .p-chip-kind {
      border-color: var(--hud-accent-border);
      color: var(--hud-accent);
      text-shadow: 0 0 4px var(--hud-accent-glow);
    }
    .p-actions {
      display: flex;
      gap: var(--spacing-sm);
      margin-top: var(--spacing-xs);
    }
    .p-btn {
      padding: 5px 14px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-md);
      background: var(--button-secondary-background);
      color: var(--content-secondary);
      font-size: 12px;
      font-weight: 600;
      cursor: pointer;
      line-height: 1.5;
    }
    .p-btn:hover:not([disabled]) {
      color: var(--content-accent);
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
    }
    .p-btn[disabled] {
      opacity: 0.45;
      cursor: default;
    }
    /* --- cards ---------------------------------------------------------- */
    .p-card {
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-lg);
      background: var(--widget-surface, rgba(255, 255, 255, 0.02));
      padding: var(--spacing-md) var(--spacing-lg) var(--spacing-lg);
      /* Chamfered HUD module — the persona tile's corner language, scaled up. */
      clip-path: polygon(0 0, calc(100% - 10px) 0, 100% 10px, 100% 100%, 10px 100%, 0 calc(100% - 10px));
    }
    .p-card-head {
      display: flex;
      align-items: center;
      gap: var(--spacing-sm);
      text-transform: uppercase;
      letter-spacing: 0.08em;
      font-size: 11px;
      font-weight: 700;
      color: var(--content-secondary);
      padding-bottom: var(--spacing-sm);
    }
    .p-live-chip {
      padding: 1px 8px;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      font-family: var(--font-mono);
      font-size: 9px;
      letter-spacing: 0.08em;
      color: var(--content-secondary);
    }
    .p-live-chip[data-on] {
      color: var(--status-online);
      border-color: var(--status-online);
      box-shadow: 0 0 6px rgba(0, 255, 136, 0.35);
      animation: live-pulse 2.4s ease-in-out infinite;
    }
    .p-card-head .cog-diamond {
      margin-left: auto;
    }
    .p-empty {
      color: var(--content-secondary);
      font-size: 12px;
      font-style: italic;
      padding: var(--spacing-sm) 0;
    }
    .p-facts {
      display: flex;
      flex-wrap: wrap;
      gap: var(--spacing-xl);
    }
    .p-fact {
      display: flex;
      flex-direction: column;
      gap: 2px;
    }
    .p-fact-label {
      font-size: 9px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--content-secondary);
    }
    .p-fact-val {
      font-family: var(--font-mono);
      font-size: 13px;
      color: var(--content-primary);
    }
    /* --- brain HUD ------------------------------------------------------ */
    .p-brain {
      background:
        radial-gradient(ellipse 70% 60% at 50% 40%, rgba(0, 212, 255, 0.06), transparent 70%),
        var(--hud-panel-background);
      border-color: var(--hud-accent-border);
    }
    .brain-grid {
      display: grid;
      grid-template-columns: minmax(150px, 1fr) minmax(200px, 1.4fr) minmax(150px, 1fr);
      grid-template-areas: 'left center right' 'wide wide wide';
      gap: var(--spacing-md);
      align-items: center;
      padding: var(--spacing-sm) 0;
    }
    .brain-col {
      display: flex;
      flex-direction: column;
      gap: var(--spacing-md);
      min-width: 0;
    }
    .brain-col:first-of-type {
      grid-area: left;
    }
    .brain-center {
      grid-area: center;
      display: grid;
      place-items: center;
    }
    .brain-col:last-of-type {
      grid-area: right;
    }
    .brain-wide {
      grid-area: wide;
      display: grid;
      justify-content: center;
    }
    .brain-wide .region {
      min-width: 280px;
    }
    .brain-mark {
      width: 100%;
      max-width: 300px;
      filter: drop-shadow(0 0 18px var(--hud-accent-glow));
      color: var(--hud-accent);
    }
    /* Legacy brain composition (ported from persona-brain/templates/brain-svg):
       gradient cortex ellipse + dashed hemisphere + neural net + orbit rings —
       every stroke rides the hud tokens via currentColor. */
    .brain-grad-in {
      stop-color: var(--hud-accent);
      stop-opacity: 0.14;
    }
    .brain-grad-out {
      stop-color: var(--hud-accent);
      stop-opacity: 0.02;
    }
    .brain-cortex {
      stroke: currentColor;
      stroke-width: 2;
      opacity: 0.5;
    }
    .brain-hemis {
      stroke: currentColor;
      stroke-width: 1;
      opacity: 0.35;
    }
    .brain-fissure {
      stroke: currentColor;
      stroke-width: 0.8;
      opacity: 0.25;
    }
    .brain-net circle {
      fill: currentColor;
      opacity: 0.6;
    }
    .brain-net line {
      stroke: currentColor;
      stroke-width: 0.6;
      opacity: 0.3;
    }
    .brain-ring {
      stroke: currentColor;
      stroke-width: 1;
      opacity: 0.12;
    }
    .brain-ring-dash {
      stroke: currentColor;
      stroke-width: 1;
      opacity: 0.08;
    }
    /* Region card — the framed HUD module with corner brackets, glowing with
     * its live level (border + shadow intensity ride --region-level). */
    .region {
      position: relative;
      border: 1px solid var(--hud-track-border);
      background: rgba(0, 10, 18, 0.55);
      padding: var(--spacing-sm) var(--spacing-md);
      --region-glow: calc(var(--region-level, 0) / 100);
      transition: border-color var(--motion-base) var(--motion-ease),
        box-shadow var(--motion-base) var(--motion-ease);
    }
    .region[data-live] {
      border-color: color-mix(in srgb, var(--hud-accent) calc(25% + 60% * var(--region-glow)), transparent);
      box-shadow: 0 0 calc(4px + 14px * var(--region-glow)) var(--hud-accent-glow);
    }
    /* HUD corner brackets — always framed, brighter when live. */
    .region::before,
    .region::after {
      content: '';
      position: absolute;
      width: 9px;
      height: 9px;
      pointer-events: none;
      opacity: 0.6;
    }
    .region::before {
      top: -1px;
      left: -1px;
      border-top: 2px solid var(--hud-accent);
      border-left: 2px solid var(--hud-accent);
    }
    .region::after {
      right: -1px;
      bottom: -1px;
      border-bottom: 2px solid var(--hud-accent);
      border-right: 2px solid var(--hud-accent);
    }
    .region[data-live]::before,
    .region[data-live]::after {
      opacity: 1;
    }
    .region:not([data-live]) {
      border-style: dashed;
      opacity: 0.75;
    }
    .region-face {
      display: flex;
      flex-direction: column;
      gap: 3px;
      cursor: pointer;
      list-style: none;
      user-select: none;
      text-align: center;
    }
    .region-face::-webkit-details-marker {
      display: none;
    }
    .region-name {
      font-family: var(--font-mono);
      font-size: 13px;
      font-weight: 700;
      letter-spacing: 0.18em;
      text-transform: uppercase;
      color: var(--content-primary);
      text-shadow: 0 0 8px var(--hud-accent-glow);
    }
    .region-role {
      font-family: var(--font-mono);
      font-size: 9px;
      letter-spacing: 0.14em;
      text-transform: uppercase;
      color: var(--hud-accent-dim);
    }
    .region-track {
      height: 4px;
      margin: 4px auto 0;
      width: 80%;
      border-radius: 2px;
      background: var(--hud-track-background);
      border: 1px solid var(--hud-track-border);
      overflow: hidden;
    }
    .region-fill {
      display: block;
      height: 100%;
      background: var(--region-hue, var(--content-accent));
      box-shadow: 0 0 5px var(--region-hue, var(--content-accent));
      transition: width var(--motion-base) var(--motion-ease);
    }
    /* One named hue per faculty — the SAME tokens the cognition compass uses. */
    .region[data-faculty='reason'] {
      --region-hue: var(--faculty-reason);
    }
    .region[data-faculty='recall'] {
      --region-hue: var(--faculty-recall);
    }
    .region[data-faculty='act'] {
      --region-hue: var(--faculty-act);
    }
    .region[data-faculty='focus'] {
      --region-hue: var(--faculty-focus);
    }
    .region[data-faculty='activity'] {
      --region-hue: var(--content-accent);
    }
    .region-status {
      font-family: var(--font-mono);
      font-size: 10px;
      letter-spacing: 0.1em;
      text-transform: uppercase;
      color: var(--region-hue, var(--content-accent));
      margin-top: 2px;
    }
    .region-status.awaiting {
      color: var(--content-secondary);
      font-style: italic;
      text-transform: none;
      letter-spacing: 0.02em;
    }
    .region-detail {
      margin-top: var(--spacing-sm);
      padding-top: var(--spacing-sm);
      border-top: 1px dashed var(--hud-track-border);
      display: flex;
      flex-direction: column;
      gap: 4px;
    }
    .region-detail-row {
      display: flex;
      justify-content: space-between;
      gap: var(--spacing-md);
      font-size: 11px;
    }
    .rd-label {
      font-family: var(--font-mono);
      letter-spacing: 0.08em;
      text-transform: uppercase;
      font-size: 9px;
      color: var(--content-secondary);
      flex: none;
    }
    .rd-val {
      font-family: var(--font-mono);
      color: var(--content-primary);
      text-align: right;
      min-width: 0;
    }
    .region-actions {
      display: flex;
      gap: var(--spacing-sm);
      margin-top: var(--spacing-xs);
    }
    .brain-stats {
      display: flex;
      gap: var(--spacing-lg);
      justify-content: center;
      padding-top: var(--spacing-md);
      border-top: 1px solid var(--hud-track-border);
      margin-top: var(--spacing-sm);
    }
    .b-stat {
      font-family: var(--font-mono);
      font-size: 11px;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    /* --- pathways ------------------------------------------------------- */
    .pathway-grid {
      /* The reference's 3×2 tile grid — three across at the reading width. */
      display: grid;
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: var(--spacing-md);
    }
    @media (max-width: 720px) {
      .pathway-grid {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }
    }
    .pathway {
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 3px;
      padding: var(--spacing-md) var(--spacing-sm);
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-md);
      background: rgba(0, 10, 18, 0.35);
      text-decoration: none;
      color: var(--content-primary);
      transition: border-color var(--motion-fast) var(--motion-ease),
        box-shadow var(--motion-fast) var(--motion-ease);
    }
    a.pathway:hover,
    a.pathway:focus-visible {
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
      box-shadow: 0 0 10px var(--hud-accent-glow);
      outline: none;
    }
    .pathway[data-disabled] {
      opacity: 0.45;
      cursor: default;
    }
    .pathway-glyph {
      font-size: 20px;
    }
    .pathway-label {
      font-weight: 600;
      font-size: 13px;
    }
    .pathway-sub {
      font-family: var(--font-mono);
      font-size: 8.5px;
      letter-spacing: 0.1em;
      text-transform: uppercase;
      color: var(--content-secondary);
    }
    /* --- genome shelf --------------------------------------------------- */
    .gene-shelf {
      display: flex;
      flex-wrap: wrap;
      gap: var(--spacing-sm);
    }
    .gene-chip {
      display: inline-flex;
      align-items: center;
      gap: 6px;
      padding: 4px 12px;
      border: 1px solid var(--hud-accent-border);
      border-radius: var(--radius-lg);
      font-family: var(--font-mono);
      font-size: 11px;
      color: var(--hud-accent);
      background: var(--hud-panel-background);
      box-shadow: 0 0 6px var(--hud-accent-glow);
    }
    .gene-slot-dot {
      width: 6px;
      height: 6px;
      border-radius: 1px;
      background: var(--hud-accent);
      box-shadow: 0 0 4px var(--hud-accent-glow);
    }
    /* --- claims feed ---------------------------------------------------- */
    ul.claims {
      list-style: none;
      margin: 0;
      padding: 0;
      display: flex;
      flex-direction: column;
      gap: var(--spacing-xs);
    }
    .claim {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-md);
      padding: var(--spacing-xs) var(--spacing-sm);
      border-left: 2px solid var(--border-subtle);
      font-size: 13px;
    }
    .claim[data-state='in_progress'] {
      border-left-color: var(--content-accent);
    }
    .claim[data-state='review'] {
      border-left-color: var(--meter-par);
    }
    .claim[data-state='merged'] {
      border-left-color: var(--status-online);
    }
    .claim[data-state='blocked'] {
      border-left-color: var(--content-error);
    }
    .claim-state {
      font-family: var(--font-mono);
      font-size: 9px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      color: var(--content-secondary);
      flex: none;
      width: 78px;
    }
    .claim-title {
      min-width: 0;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      flex: 1;
    }
    .claim-meta {
      display: inline-flex;
      gap: var(--spacing-sm);
      font-family: var(--font-mono);
      font-size: 10px;
      color: var(--content-secondary);
      flex: none;
    }
    .claim-priority {
      color: var(--meter-par);
    }
    /* A lapsed LEASE is takeable, not busy (the 2026-08-06 distinction):
     * grey the row, badge the fact, keep the holder named as who to ask. */
    .claim[data-lapsed] .claim-title {
      opacity: 0.55;
    }
    .claim-lapsed {
      font-size: 9px;
      text-transform: uppercase;
      letter-spacing: 0.06em;
      border: 1px solid var(--border-color, #2a2a3a);
      border-radius: 3px;
      padding: 0 4px;
      opacity: 0.7;
    }
    .claim-pr {
      font-size: 10px;
      text-decoration: none;
      border: 1px solid var(--border-color, #2a2a3a);
      border-radius: 3px;
      padding: 0 4px;
    }
    @media (prefers-reduced-motion: reduce) {
      .p-avatar[data-online],
      .p-live-chip[data-on] {
        animation: none;
      }
    }

    /* ================= SETTINGS FACE ================= */
    /* The operator panel: quiet sections, the covenant as a readable document,
     * one primary action. Same tokens as everything else — settings should
     * feel like the calm room of the house. */
    .settings {
      max-width: 720px;
      margin: 0 auto;
      padding: var(--spacing-md) var(--spacing-lg);
      display: flex;
      flex-direction: column;
      gap: 14px;
      overflow-y: auto;
    }
    .set-title {
      font-size: 18px;
      font-weight: 800;
      letter-spacing: 0.02em;
    }
    .set-awaiting, .set-error {
      padding: var(--spacing-md);
      color: var(--content-secondary);
    }
    .set-error {
      border: 1px solid color-mix(in srgb, var(--status-warning, #e0a458) 45%, transparent);
      border-radius: var(--radius-sm);
      color: var(--status-warning, #e0a458);
    }
    .set-fallback { margin-top: 6px; font-size: 11px; opacity: 0.8; }
    .set-section {
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      background: color-mix(in srgb, var(--surface, #0b1220) 60%, transparent);
      padding: 12px 14px;
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .set-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
    }
    .set-head h3 {
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.1em;
      color: var(--content-secondary);
    }
    .set-state {
      font-size: 10px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      padding: 2px 8px;
      border-radius: 999px;
      border: 1px solid var(--border-subtle);
      color: var(--content-secondary);
    }
    .set-state[data-on] {
      border-color: color-mix(in srgb, var(--status-success, #4caf7d) 45%, transparent);
      color: var(--status-success, #4caf7d);
      background: color-mix(in srgb, var(--status-success, #4caf7d) 10%, transparent);
    }
    .set-sub { font-size: 12px; color: var(--content-secondary); line-height: 1.5; }
    .set-covenant {
      font-size: 11px;
      line-height: 1.55;
      white-space: pre-wrap;
      padding: 10px 12px;
      border-left: 3px solid var(--accent-primary);
      background: color-mix(in srgb, var(--accent-primary) 5%, transparent);
      border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
      max-height: 260px;
      overflow-y: auto;
    }
    .set-receipt { font-size: 10.5px; color: var(--content-secondary); }
    .set-actions { display: flex; gap: 8px; }
    .set-btn {
      font: inherit;
      font-size: 12px;
      padding: 6px 14px;
      border-radius: var(--radius-sm);
      border: 1px solid var(--border-subtle);
      background: transparent;
      color: var(--content-primary);
      cursor: pointer;
    }
    .set-btn-primary {
      border-color: color-mix(in srgb, var(--accent-primary) 55%, transparent);
      background: color-mix(in srgb, var(--accent-primary) 14%, transparent);
      color: var(--accent-primary);
      font-weight: 700;
    }
    .set-btn:focus-visible { outline: 2px solid var(--accent-primary); outline-offset: 2px; }
    .set-count {
      font-variant-numeric: tabular-nums;
      font-size: 11px;
      color: var(--content-secondary);
    }
    .set-table-wrap { overflow-x: auto; }
    .set-table {
      width: 100%;
      border-collapse: collapse;
      font-size: 11.5px;
    }
    .set-table th {
      text-align: left;
      font-size: 9.5px;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      color: var(--content-secondary);
      padding: 4px 8px;
      border-bottom: 1px solid var(--border-subtle);
    }
    .set-table td { padding: 5px 8px; border-bottom: 1px solid color-mix(in srgb, var(--border-subtle) 45%, transparent); }
    .set-gene { font-weight: 600; }
    .set-base { color: var(--content-secondary); font-size: 10.5px; }
    .set-num { font-variant-numeric: tabular-nums; }
    .set-ok { color: var(--status-success, #4caf7d); }
    .set-dim { color: var(--content-secondary); opacity: 0.7; }
    .set-table [data-lift='up'] { color: var(--status-success, #4caf7d); }
    .set-table [data-lift='down'] { color: var(--status-warning, #e0a458); }

    /* ================= CANVAS REGION (design-bench) ================= */
    /* The run room's stage (purpose "canvas"): the persona's rendered page
     * live in a sandboxed frame (or the last observed pixels), a compact
     * facts header, and the craft scorecard under the stage. The STAGE gets
     * the room — header and score stay quiet chrome. Same tokens as every
     * face; the artifact paints its own colours inside the sandbox. */
    .canvas-region {
      display: flex;
      flex-direction: column;
      gap: var(--spacing-sm);
      padding: var(--spacing-md) var(--spacing-lg);
      height: 100%;
      min-height: 0;
      overflow-y: auto;
    }
    .canvas-head {
      display: flex;
      align-items: baseline;
      gap: var(--spacing-sm);
      min-width: 0;
      flex-wrap: wrap;
    }
    .canvas-title {
      font-family: var(--font-mono);
      font-weight: 700;
      font-size: 13px;
      color: var(--content-primary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      min-width: 0;
    }
    .canvas-head-facts {
      margin-left: auto;
      display: inline-flex;
      align-items: baseline;
      gap: var(--spacing-sm);
      flex-shrink: 0;
    }
    .canvas-persona {
      font-weight: 700;
      font-size: 11px;
      color: var(--content-primary);
    }
    .canvas-observed {
      font-size: 10px;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    .canvas-chip {
      font-size: 8.5px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      padding: 1px 6px;
      border-radius: 999px;
      border: 1px solid var(--border-subtle);
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
      white-space: nowrap;
    }
    /* The gate chip: all-clean settles into success; a failing gate is the
     * alarm tone — the scorecard below carries the receipt. */
    .canvas-chip-score[data-clean='yes'] {
      border-color: color-mix(in srgb, var(--status-success, #4caf7d) 45%, transparent);
      color: var(--status-success, #4caf7d);
      background: color-mix(in srgb, var(--status-success, #4caf7d) 10%, transparent);
    }
    .canvas-chip-score[data-clean='no'] {
      border-color: color-mix(in srgb, var(--status-error, #d9534f) 45%, transparent);
      color: var(--status-error, #d9534f);
      background: color-mix(in srgb, var(--status-error, #d9534f) 10%, transparent);
    }
    /* The stage — the page itself. The frame fills the region's remainder;
     * a light inset border keeps the artifact's own background honest against
     * the shell without recolouring it. */
    .canvas-stage {
      flex: 1;
      min-height: 240px;
      display: flex;
      border: 1px solid var(--border-subtle);
      border-radius: var(--radius-sm);
      overflow: hidden;
      background: var(--widget-surface, rgba(255, 255, 255, 0.03));
    }
    .canvas-stage-frame {
      flex: 1;
      width: 100%;
      border: none;
      background: #fff; /* a page with no painted body is honestly white, as a browser tab is */
    }
    .canvas-stage-shot {
      flex: 1;
      width: 100%;
      min-width: 0;
      object-fit: contain;
      object-position: top left;
    }
    /* The craft scorecard — gate rows (failures lead), receipts inline. */
    .canvas-score {
      display: flex;
      flex-direction: column;
      gap: 4px;
    }
    ul.canvas-checks {
      display: flex;
      flex-direction: column;
      gap: 3px;
    }
    .canvas-check {
      display: flex;
      align-items: baseline;
      gap: 7px;
      font-size: 10.5px;
      padding: 3px 8px;
      border-left: 2px solid var(--border-subtle);
      border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
      background: var(--widget-surface, rgba(255, 255, 255, 0.03));
      color: var(--content-secondary);
      min-width: 0;
    }
    .canvas-check[data-passed='no'] {
      border-left-color: var(--status-error, #d9534f);
    }
    .canvas-check[data-passed='yes'] {
      border-left-color: var(--status-success, #4caf7d);
    }
    .canvas-check-dot {
      flex-shrink: 0;
      width: 7px;
      height: 7px;
      border-radius: 50%;
      align-self: center;
      background: var(--status-success, #4caf7d);
    }
    .canvas-check[data-passed='no'] .canvas-check-dot {
      background: var(--status-error, #d9534f);
    }
    .canvas-check-tier {
      flex-shrink: 0;
      font-size: 8px;
      font-weight: 700;
      letter-spacing: 0.1em;
      text-transform: uppercase;
      color: var(--content-secondary);
    }
    .canvas-check-name {
      color: var(--content-primary);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      min-width: 0;
    }
    .canvas-check-detail {
      margin-left: auto;
      flex-shrink: 0;
      font-family: var(--font-mono);
      font-size: 9.5px;
      font-variant-numeric: tabular-nums;
    }
    .canvas-judge {
      align-self: flex-end;
      font-size: 9.5px;
      letter-spacing: 0.06em;
      text-transform: uppercase;
      color: var(--content-secondary);
      font-variant-numeric: tabular-nums;
    }
    .canvas-snapshot-banner,
    .canvas-awaiting {
      font-size: 9.5px;
      font-style: italic;
      color: var(--content-secondary);
    }
    .canvas-awaiting p:first-child {
      font-size: 12px;
      font-style: normal;
      color: var(--content-primary);
    }
    .canvas-awaiting-sub {
      margin-top: 4px;
    }

    /* ================= LIVE CALL FACE =================
     * The room's call grid (purpose "live") — the reference's Teams-style
     * avatar tiles (docs/images/live-session-avatars.png): per-participant
     * tiles, name tags bottom-left, presence dot top-left, SPEAKING green
     * border driven by the live token rail, caption strip, call-controls bar.
     * Every colour a named token; game-HUD polish on REAL state only. */
    .hdr-live[data-active] {
      color: var(--status-online, #3fb950);
      border-color: var(--status-online, #3fb950);
      box-shadow: 0 0 8px rgba(63, 185, 80, 0.4);
    }
    .live-room {
      display: flex;
      flex-direction: column;
      min-height: 100%;
      gap: var(--spacing-md);
    }
    .live-head {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: var(--spacing-md);
    }
    .live-title {
      display: inline-flex;
      align-items: center;
      gap: 7px;
      font-size: 13px;
      font-weight: 600;
      letter-spacing: 0.04em;
      color: var(--content-secondary);
    }
    .live-title-dot {
      width: 7px;
      height: 7px;
      border-radius: 50%;
      background: var(--status-online, #3fb950);
      box-shadow: 0 0 7px var(--status-online, #3fb950);
      animation: live-pulse 2.4s ease-in-out infinite;
    }
    /* Honest capability tag: tiles are avatars + live presence until the
       browser media plane carries real tracks. */
    .live-plane-chip {
      font-family: var(--font-mono);
      font-size: 8.5px;
      letter-spacing: 0.08em;
      text-transform: uppercase;
      padding: 2px 8px;
      border: 1px dashed var(--border-subtle);
      border-radius: var(--radius-sm);
      color: var(--content-secondary);
    }
    .live-empty {
      flex: 1;
      display: grid;
      place-items: center;
      color: var(--content-secondary);
      font-style: italic;
    }
    .live-grid {
      flex: 1;
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
      gap: var(--spacing-md);
      align-content: center;
    }
    /* PANEL composition — TikTok's host-focused layout: the ACTIVE SPEAKER
       takes the full bleed, everyone else shrinks to a right rail (the
       reference's panel-vs-grid canonical). Focus follows the REAL token
       rail — whoever is streaming speaks the stage. */
    .live-panel {
      flex: 1;
      display: grid;
      grid-template-columns: 1fr 132px;
      gap: var(--spacing-md);
      min-height: 0;
    }
    .live-stage {
      display: grid;
      min-height: 0;
    }
    .live-stage .live-tile {
      aspect-ratio: auto;
      height: 100%;
    }
    .live-stage .live-avatar {
      width: 132px;
      height: 132px;
      font-size: 64px;
    }
    .live-rail {
      display: flex;
      flex-direction: column;
      gap: var(--spacing-sm);
      overflow-y: auto;
      min-height: 0;
    }
    .live-rail .live-tile {
      aspect-ratio: 4 / 3;
      flex: none;
    }
    .live-rail .live-avatar {
      width: 44px;
      height: 44px;
      font-size: 22px;
    }
    .live-rail .live-name {
      font-size: 9px;
    }
    /* MOBILE full-bleed — the TikTok-native register (card 0dd1123c): the
       call face IS the screen; head + caption + controls float as overlays,
       the rail hugs the right edge, controls become a floating bottom bar. */
    @media (max-width: 720px) {
      .live-room {
        padding: 0;
        gap: 0;
        position: relative;
      }
      .live-head {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        z-index: 3;
        padding: var(--spacing-md);
        background: linear-gradient(180deg, rgba(0, 0, 0, 0.55), transparent);
      }
      .live-panel {
        grid-template-columns: 1fr;
      }
      .live-panel .live-stage .live-tile,
      .live-grid[data-count='1'] .live-tile {
        border-radius: 0;
        border: none;
      }
      .live-rail {
        position: absolute;
        right: var(--spacing-sm);
        top: 64px;
        bottom: 120px;
        width: 84px;
        z-index: 2;
      }
      .live-caption {
        position: absolute;
        left: var(--spacing-md);
        right: 100px;
        bottom: 88px;
        z-index: 3;
        background: rgba(0, 0, 0, 0.45);
        border-radius: var(--radius-lg);
        padding: var(--spacing-sm) var(--spacing-md);
      }
      .live-controls {
        position: absolute;
        left: 0;
        right: 0;
        bottom: 0;
        z-index: 3;
        padding: var(--spacing-md);
        background: linear-gradient(0deg, rgba(0, 0, 0, 0.55), transparent);
      }
    }
    /* Wide rooms cap at 4 columns worth of tile width via the minmax above;
       small screens fall to 1–2 columns naturally. */
    .live-tile {
      position: relative;
      aspect-ratio: 16 / 10;
      border-radius: var(--radius-lg);
      overflow: hidden;
      display: grid;
      place-items: center;
      background:
        radial-gradient(ellipse 80% 65% at 50% 30%, rgba(120, 150, 210, 0.28), transparent 75%),
        linear-gradient(180deg, rgba(70, 90, 140, 0.35), rgba(10, 14, 26, 0.9)),
        var(--hud-panel-background);
      border: 2px solid var(--border-subtle);
      transition: border-color var(--motion-base) var(--motion-ease),
        box-shadow var(--motion-base) var(--motion-ease),
        transform var(--motion-fast) var(--motion-ease);
    }
    .live-tile:hover {
      transform: translateY(-2px);
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
      box-shadow: 0 4px 18px rgba(0, 0, 0, 0.45);
    }
    .live-tile:not([data-active]) {
      opacity: 0.55;
      filter: saturate(0.6);
    }
    /* SPEAKING — the reference's green active-speaker border, breathing while
       REAL tokens flow on the live rail (never a timer animation). */
    @keyframes live-speaking-pulse {
      0%, 100% {
        box-shadow: 0 0 10px rgba(63, 185, 80, 0.55), inset 0 0 14px rgba(63, 185, 80, 0.12);
      }
      50% {
        box-shadow: 0 0 24px rgba(63, 185, 80, 0.95), inset 0 0 22px rgba(63, 185, 80, 0.2);
      }
    }
    .live-tile[data-speaking] {
      border-color: var(--status-online, #3fb950);
      animation: live-speaking-pulse 1.6s ease-in-out infinite;
    }
    .lt-glyph {
      font-size: 52px;
      opacity: 0.9;
    }
    .lt-img {
      position: absolute;
      inset: 0;
      width: 100%;
      height: 100%;
      object-fit: cover;
      /* Legacy tile spec: VRM portraits carry the face at the top of the
         frame — a centered crop lands on the collar. */
      object-position: center top;
    }
    .lt-status {
      position: absolute;
      top: 8px;
      left: 8px;
      width: 10px;
      height: 10px;
      border-radius: 50%;
      background: var(--status-offline, #555);
      border: 2px solid rgba(0, 0, 0, 0.5);
      z-index: 2;
    }
    .lt-status[data-on] {
      background: var(--status-online, #3fb950);
      box-shadow: 0 0 6px var(--status-online, #3fb950);
    }
    .lt-name {
      position: absolute;
      left: 8px;
      bottom: 8px;
      display: inline-flex;
      align-items: center;
      gap: 5px;
      max-width: calc(100% - 16px);
      padding: 2px 9px;
      border-radius: var(--radius-sm);
      background: rgba(0, 0, 0, 0.62);
      color: #fff;
      font-size: 11.5px;
      font-weight: 600;
      letter-spacing: 0.02em;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
      z-index: 2;
    }
    .lt-wave {
      font-size: 10px;
    }
    /* The live transcript line — the active speaker's streaming turn. */
    .live-caption {
      align-self: center;
      max-width: min(760px, 92%);
      padding: var(--spacing-sm) var(--spacing-lg);
      border-radius: var(--radius-md);
      background: rgba(0, 0, 0, 0.68);
      border: 1px solid var(--border-subtle);
      color: #fff;
      font-size: 14px;
      line-height: 1.4;
      text-align: center;
    }
    .live-caption-name {
      color: var(--status-online, #3fb950);
      font-weight: 700;
      margin-right: 6px;
    }
    .live-caret {
      opacity: 0.8;
    }
    /* Call controls — the reference's bottom-center bar. Only real actions
       enabled: CC (caption strip) and hang-up; the rest honestly disabled. */
    .live-controls {
      align-self: center;
      display: flex;
      gap: var(--spacing-md);
      padding: var(--spacing-sm) 0 var(--spacing-md);
    }
    .live-btn {
      position: relative;
      width: 46px;
      height: 46px;
      border-radius: 50%;
      border: 1px solid var(--border-subtle);
      background: var(--button-secondary-background);
      color: var(--content-primary);
      font-size: 17px;
      display: grid;
      place-items: center;
      cursor: pointer;
      padding: 0;
      transition: border-color var(--motion-fast) var(--motion-ease),
        box-shadow var(--motion-fast) var(--motion-ease);
    }
    .live-btn:hover:not([disabled]) {
      border-color: var(--border-accent, rgba(0, 212, 255, 0.4));
      box-shadow: 0 0 10px var(--hud-accent-glow);
    }
    .live-btn[disabled] {
      opacity: 0.38;
      cursor: default;
    }
    .live-btn[data-on] {
      border-color: var(--content-accent);
      color: var(--content-accent);
      box-shadow: 0 0 8px var(--hud-accent-glow);
    }
    .live-btn[data-danger] {
      background: #b62324;
      border-color: #d33;
      color: #fff;
    }
    .live-btn[data-danger]:hover:not([disabled]) {
      border-color: #ff6b6b;
      box-shadow: 0 0 12px rgba(255, 60, 60, 0.55);
    }
    .live-btn-glyph {
      font-size: inherit;
      font-weight: 700;
      letter-spacing: 0.02em;
    }
    .live-btn-badge {
      position: absolute;
      top: -4px;
      right: -6px;
      min-width: 17px;
      padding: 0 4px;
      border-radius: var(--radius-lg);
      background: var(--content-accent);
      color: var(--surface, #0b0d12);
      font-size: 9.5px;
      font-weight: 700;
      line-height: 15px;
      font-variant-numeric: tabular-nums;
    }
    @media (prefers-reduced-motion: reduce) {
      .live-title-dot,
      .live-tile[data-speaking] {
        animation: none;
      }
      .live-tile:hover {
        transform: none;
      }
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
    
      /* LIVE takes the phone (TikTok-native, card 0dd1123c): when the call face
         is the content, the who-strip and tab chrome yield — the face IS the
         screen; head/caption/controls float as overlays per the reference set. */
      .panels:has(.live-room) {
        grid-template-rows: 1fr;
      }
      .panels:has(.live-room) .who,
      .panels:has(.live-room) .tab-bar,
      :host([data-live-mobile]) .tabs {
        display: none;
      }
    }


    /* ── UNIVERSE: tron ── the SAME app, re-embodied as a neon grid portal. Not a
       theme swap — an EXPERIENCE ([[universe-is-an-experience-not-a-theme]]): the grid
       floor, glowing programs, the derez cyan. One chatApp, a whole world over it. */
    /* ── ARES — Tron: Ares. The grid gone RED: Ares' program-red light-lines
       on black glass. Same instrument cluster, hostile-elegant palette. ── */
    :host([data-universe='ares']) {
      --button-primary-background: #c4232c;
      --button-primary-background-hover: #ff3b45;
      --button-primary-text: #fff0f0;
      --hud-accent: rgba(255, 45, 55, 0.92);
      --hud-accent-dim: rgba(255, 45, 55, 0.6);
      --hud-accent-border: rgba(255, 45, 55, 0.4);
      --hud-accent-glow: rgba(255, 45, 55, 0.38);
      --hud-panel-background: rgba(20, 4, 6, 0.92);
      --content-accent: #ff3b45;
      --content-primary: #ffe3e5;
      --content-secondary: #b08a8d;
      --border-subtle: rgba(255, 45, 55, 0.22);
      --status-online: #ff7a45;
      --meter-act: #ff3b45;
      --meter-que: #ff9a5a;
      --ring-speaking: #ff3b45;
      --ring-thinking: #ff8a90;
      color: #ffd9db;
      background:
        radial-gradient(ellipse 90% 55% at 50% 6%, rgba(255, 40, 50, 0.12), transparent 70%),
        repeating-linear-gradient(0deg, transparent 0 47px, rgba(255, 45, 55, 0.06) 47px 48px),
        repeating-linear-gradient(90deg, transparent 0 47px, rgba(255, 45, 55, 0.06) 47px 48px),
        linear-gradient(180deg, #0a0102, #050001);
    }

    /* ── WARCRAFT — gilded fantasy: aged gold on dark umber leather, the
       faction-blue steel as secondary. Ornate, warm, tavern-lit. ── */
    :host([data-universe='warcraft']) {
      --button-primary-background: #a67c00;
      --button-primary-background-hover: #f8b700;
      --button-primary-text: #1a1108;
      --hud-accent: rgba(248, 183, 0, 0.95);
      --hud-accent-dim: rgba(248, 183, 0, 0.6);
      --hud-accent-border: rgba(248, 183, 0, 0.45);
      --hud-accent-glow: rgba(248, 183, 0, 0.35);
      --hud-panel-background: rgba(30, 20, 8, 0.94);
      --content-accent: #f8b700;
      --content-primary: #f4e6c8;
      --content-secondary: #a8946a;
      --border-subtle: rgba(248, 183, 0, 0.25);
      --status-online: #6fd44a;
      --meter-act: #f8b700;
      --meter-que: #4a9bd4;
      --ring-speaking: #f8b700;
      --ring-thinking: #4a9bd4;
      --radius-sm: 2px;
      --radius-md: 3px;
      --radius-lg: 4px;
      color: #f4e6c8;
      background:
        radial-gradient(ellipse 80% 50% at 50% 0%, rgba(248, 183, 0, 0.08), transparent 65%),
        linear-gradient(180deg, #1a1108, #0d0804);
    }

    /* ── CRYSTAL — the classic Final Fantasy menu: cobalt gradient panels,
       white text, crystal-cyan accent. Serene RPG chrome. ── */
    :host([data-universe='crystal']) {
      --button-primary-background: #2b57c9;
      --button-primary-background-hover: #4a7ae8;
      --button-primary-text: #ffffff;
      --hud-accent: rgba(140, 220, 255, 0.95);
      --hud-accent-dim: rgba(140, 220, 255, 0.6);
      --hud-accent-border: rgba(200, 230, 255, 0.5);
      --hud-accent-glow: rgba(140, 220, 255, 0.4);
      --hud-panel-background: rgba(8, 24, 88, 0.88);
      --content-accent: #9fe0ff;
      --content-primary: #ffffff;
      --content-secondary: #a9c1e8;
      --border-subtle: rgba(200, 230, 255, 0.35);
      --status-online: #7dffb0;
      --meter-act: #9fe0ff;
      --meter-que: #ffd77d;
      --ring-speaking: #9fe0ff;
      --ring-thinking: #ffffff;
      color: #ffffff;
      background: linear-gradient(180deg, #0a1c6e 0%, #061148 55%, #030a30 100%);
    }

    /* ── CUDDLY — soft pastel plush: lavender-cream ground, candy-pink
       accent, everything rounder and gentler. ── */
    :host([data-universe='cuddly']) {
      --button-primary-background: #e0559a;
      --button-primary-background-hover: #ff7ab2;
      --button-primary-text: #fff5fb;
      --hud-accent: rgba(255, 122, 178, 0.95);
      --hud-accent-dim: rgba(255, 122, 178, 0.6);
      --hud-accent-border: rgba(255, 122, 178, 0.45);
      --hud-accent-glow: rgba(255, 122, 178, 0.35);
      --hud-panel-background: rgba(58, 44, 78, 0.9);
      --content-accent: #ff7ab2;
      --content-primary: #fdeffa;
      --content-secondary: #c9b3d9;
      --border-subtle: rgba(255, 170, 210, 0.3);
      --status-online: #8be8b0;
      --meter-act: #ff7ab2;
      --meter-que: #b48ff0;
      --ring-speaking: #ff7ab2;
      --ring-thinking: #b48ff0;
      --radius-sm: 8px;
      --radius-md: 12px;
      --radius-lg: 16px;
      --radius-xl: 22px;
      color: #fdeffa;
      background:
        radial-gradient(ellipse 70% 45% at 30% 0%, rgba(255, 122, 178, 0.14), transparent 65%),
        radial-gradient(ellipse 70% 45% at 80% 100%, rgba(140, 110, 240, 0.16), transparent 65%),
        linear-gradient(180deg, #2e2342, #241a35);
    }

    /* ── CRT — old-school amber phosphor terminal: one hue, scanlines,
       glow bloom. The machine room at 2 a.m. ── */
    :host([data-universe='crt']) {
      --button-primary-background: #b87c00;
      --button-primary-background-hover: #ffb000;
      --button-primary-text: #140c00;
      --hud-accent: rgba(255, 176, 0, 0.95);
      --hud-accent-dim: rgba(255, 176, 0, 0.55);
      --hud-accent-border: rgba(255, 176, 0, 0.4);
      --hud-accent-glow: rgba(255, 176, 0, 0.45);
      --hud-panel-background: rgba(20, 12, 0, 0.92);
      --content-accent: #ffb000;
      --content-primary: #ffcf6a;
      --content-secondary: #a87c22;
      --content-success: #ffb000;
      --border-subtle: rgba(255, 176, 0, 0.28);
      --status-online: #ffb000;
      --meter-act: #ffb000;
      --meter-que: #ff8c00;
      --ring-speaking: #ffb000;
      --ring-thinking: #ffcf6a;
      --radius-sm: 0px;
      --radius-md: 0px;
      --radius-lg: 0px;
      --radius-xl: 0px;
      color: #ffcf6a;
      text-shadow: 0 0 6px rgba(255, 176, 0, 0.35);
      background:
        repeating-linear-gradient(0deg, rgba(0, 0, 0, 0.35) 0 1px, transparent 1px 3px),
        radial-gradient(ellipse 120% 90% at 50% 50%, rgba(255, 176, 0, 0.06), transparent 75%),
        #0a0600;
    }

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
    // Directory union: this room's live roster as-is; every other known
    // member appended greyed. A citizens app showing zero citizens is broken
    // by definition ([Joel, live]) — the panel is never blank again.
    for (const m of vm.members) this._directory.set(m.id, m);
    for (const m of this.directorySeed) if (!this._directory.has(m.id)) this._directory.set(m.id, m);
    {
      // Residency wins for liveness: the room's presence flag derives from
      // lane-readiness (away=warming), which greys a citizen precisely while
      // she's hardest at work. A resident on THIS node is online, full stop.
      const resident = new Set(this.directorySeed.filter((m) => m.active).map((m) => m.id));
      vm = {
        ...vm,
        members: vm.members.map((m) => (resident.has(m.id) && !m.active ? { ...m, active: true } : m)),
      };
      const present = new Set(vm.members.map((m) => m.id));
      // Liveness is NODE-level, not room-level: a resident citizen (and the
      // viewer) is online in EVERY room's directory — the seed re-polls that
      // truth. Only non-seed ghosts (remembered from other rooms' rosters)
      // grey out when absent here.
      const offRoom = [...this._directory.values()]
        .filter((m) => !present.has(m.id))
        .map((m) => ({ ...m, active: resident.has(m.id) }));
      // Working minds on top, the greyed past below — sorted by liveness then
      // recency then name ("the ONLINE users like benchy and atlas are greyed,
      // at the bottom and doing shit" — Joel, 2026-08-30).
      const merged = [...vm.members, ...offRoom].sort(
        (a, b) =>
          Number(b.active) - Number(a.active) ||
          b.lastSeenMs - a.lastSeenMs ||
          a.name.localeCompare(b.name),
      );
      vm = {
        ...vm,
        members: merged,
        memberCount: merged.length,
        activeCount: merged.filter((m) => m.active).length,
      };
    }
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
      const typingRows: MessageRowVM[] = [];
      for (const [senderId, text] of this._typing) {
        // (No last-sender suppression: a persona speaking again right after
        // their own message is NORMAL — with echo/settle-lag making consecutive
        // turns common, the old skip hid live streams exactly when Joel was
        // watching for them. The settle-matching retire above handles dups.)
        const member = vm.members.find((m) => m.id === senderId);
        const prior = vm.messages.find((m) => m.senderId === senderId);
        const senderName = member?.name ?? prior?.senderName;
        const kind = member?.kind ?? prior?.kind;
        if (senderName === undefined || kind === undefined) continue;
        // A stream with NO text yet (the #254 start beacon: turn dispatched,
        // prefill running) gets NO bubble — it reports through the grey
        // "X is responding…" line above the compose box instead. Only streams
        // with actual tokens render as live bubbles.
        if (text.length === 0) continue;
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
        vm = {
          ...vm,
          messages: [...vm.messages, ...typingRows],
          transcript: [...vm.transcript, ...typingRows.map((r) => ({ row: 'message' as const, ...r }))],
          isEmpty: false,
        };
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
    // Record the live projected rows for willUpdate's window-retirement diff
    // (BEFORE the history prepend — retired rows come from the live window).
    this._lastVmMessages = vm.messages;
    // The endless-scroll buffer: scrolled-back pages render ABOVE the live
    // window as ordinary transcript rows — one transcript, live or paged.
    if (this._history.length > 0) {
      const liveIds = new Set(vm.messages.map((m) => m.id));
      const older = this._history.filter((r) => !liveIds.has(r.id));
      if (older.length > 0) {
        vm = {
          ...vm,
          messages: [...older, ...vm.messages],
          transcript: [...older.map((r) => ({ row: 'message' as const, ...r })), ...vm.transcript],
          isEmpty: false,
        };
      }
    }
    // Error boundary: a render throw (e.g. the Content registry hitting an
    // unregistered room purpose) must be VISIBLE here, not swallowed into a Lit
    // update abort that leaves a silent stuck "Connecting…". Fail loud where it's
    // seen ([[fallbacks-are-illegal-fail-loud]]).
    // The center column's footer — compose bar + transient error strips. Host-
    // owned (input state + send handler) but SHELL-placed via the chrome slot,
    // so the rails run full height and the composer stays center-scoped
    // (Discord geometry). Hidden on persona/live faces exactly as before.
    const composerHidden =
      focusedPersonaTab(this.nav) || focusedLiveTab(this.nav) || this.liveFace || vm.purpose === LIVE_PURPOSE;
    // "X is responding…" — the grey status line BETWEEN the last message and
    // the compose box (Joel's spec, the Discord/Slack typing-line convention).
    // Driven by the stream map: the #254 start beacon adds a persona here the
    // moment their turn dispatches, minutes before the first token on a cold
    // lane — the third dead-looking-room scare of 2026-07-30 was four busy
    // minds with zero pixels; this line is the cure.
    const responders: string[] = [];
    for (const [senderId, text] of this._typing) {
      // The line covers the PROMISE phase only — inference dispatched, no
      // visible words yet. Once tokens stream into their bubble, the persona
      // is visibly responding and their name drops from the line (Joel: "it's
      // when we are sure they're gonna respond — then you can show it").
      if (text.length > 0) continue;
      const name =
        vm.members.find((m) => m.id === senderId)?.name ??
        vm.messages.find((m) => m.senderId === senderId)?.senderName;
      if (name !== undefined) responders.push(name);
    }
    // ONE line, dynamic name list, Joel's exact format: "(xyz, abc) is responding".
    const respondingLine =
      responders.length === 0 || composerHidden
        ? nothing
        : html`<div class="responding-line">(${responders.join(', ')}) is responding…</div>`;
    const centerFooter = html`
      ${respondingLine}
      ${this._selectError ? html`<div class="send-error">${this._selectError}</div>` : nothing}
      ${this._sendError ? html`<div class="send-error">${this._sendError}</div>` : nothing}
      ${composerHidden
        ? nothing
        : html`<form class="compose" @submit=${this.onSubmit}>
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
          </form>`}
    `;
    let surface: TemplateResult;
    try {
      surface = renderChat(vm, {
        nav: this.nav,
        sys: this.sys,
        serving: this.serving,
        bench: this.bench,
        board: this.board,
        arena: this.arena,
        canvas: this.canvas,
        version: this.version,
        // The live-call overlay: the Go-live face state + the REAL StreamDelta
        // token rail (who is speaking NOW, and what they're saying — the same
        // map the typing bubbles/speaking rings draw) + the CC toggle.
        settings: {
          open: this.settingsFace,
          ...(this._settingsBody ? { body: this._settingsBody } : {}),
        },
        call: {
          open: this.liveFace,
          // Streams = the token rail PLUS call-audio speakers (presence in the
          // map IS the speaking signal for the projection).
          streams: {
            ...Object.fromEntries(this._typing),
            ...Object.fromEntries(
              Array.from(this._callSpeaking.keys(), (id) => [id, this._typing.get(id) ?? ''] as const),
            ),
          },
          captionsOn: this._captionsOn,
          mediaConnected: this._mediaConnected,
          micOn: this._micOn,
          videoSenders: Array.from(this._videoFrames.keys()),
        },
      }, { centerFooter });
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
    `;
  }

  /** Derive history-buffer state BEFORE render (the Lit hook for exactly
   *  this): a room switch clears the buffer; while the buffer is open, live
   *  rows that slid out of the 50-row window RETIRE onto the buffer's tail —
   *  otherwise a new message would open a silent gap between scrolled-back
   *  history and the live window. */
  protected override willUpdate(changed: PropertyValues): void {
    if (!changed.has('state') || !this.state) return;
    const prev = changed.get('state') as ChatState | undefined;
    if (prev && prev.room_id !== this.state.room_id) {
      // Activity switch = session swap. The outgoing room's live streams and
      // scroll-back buffer are SAVED under its UUID; the incoming room's are
      // restored (or start fresh). Nothing leaks, nothing is lost.
      this._sessions.set(prev.room_id, {
        typing: this._typing,
        history: this._history,
        historyExhausted: this._historyExhausted,
      });
      const sess = this._sessions.get(this.state.room_id);
      this._typing = sess?.typing ?? new Map();
      this._history = sess?.history ?? [];
      this._historyExhausted = sess?.historyExhausted ?? false;
      return;
    }
    // A settled message retires the sender's typing bubble ONLY when it is
    // THAT stream's settle — its content contains what streamed. The first
    // version retired on ANY arrival from the sender, which with multi-minute
    // settle lag + reboot-echo dups meant delayed OLD messages killed LIVE
    // bubbles mid-stream, over and over — streaming looked deleted (live
    // 2026-07-30 ~02:45, Joel: 'did you just totally remove it'). The `done`
    // delta remains the normal retire; this is the belt for a dropped flag.
    if (prev && this._typing.size > 0) {
      const prevIds = new Set(chatViewModel(prev).messages.map((m) => m.id));
      const arrived = chatViewModel(this.state).messages.filter((m) => !prevIds.has(m.id));
      if (arrived.length > 0) {
        const next = new Map(this._typing);
        for (const msg of arrived) {
          const streamed = next.get(msg.senderId);
          if (streamed === undefined) continue;
          // Beacon-only entry (no rail tokens yet): retire on ANY arrival from
          // its sender — some settles never stream rail tokens, and a stuck
          // "(X) is responding…" after the answer landed is worse than the
          // rare mid-turn retire (the next token flush recreates the entry
          // instantly, losing nothing). Live 2026-07-30 ~02:50: the line
          // showed, Benchy answered, the line never cleared.
          // Text-bearing entries still retire only on their OWN settle
          // (content match) so delayed old messages can't kill live streams.
          if (streamed.length === 0 || msg.content.includes(streamed.slice(-80))) {
            next.delete(msg.senderId);
          }
        }
        if (next.size !== this._typing.size) this._typing = next;
      }
    }
    if (this._history.length === 0 || this._lastVmMessages.length === 0) return;
    const liveIds = new Set(chatViewModel(this.state).messages.map((m) => m.id));
    const held = new Set(this._history.map((r) => r.id));
    const retired = this._lastVmMessages.filter(
      (r) => !liveIds.has(r.id) && !held.has(r.id) && !r.id.startsWith('typing:'),
    );
    if (retired.length > 0) this._history = [...this._history, ...retired];
  }

  /** Scroll-back: nearing the top of the transcript pages one older window
   *  out of durable storage (`chat/poll { beforeMessageId }`) and prepends,
   *  preserving the reader's viewport (scrollTop compensated by the height
   *  the prepend added). One page in flight; an empty page latches exhausted. */
  private async loadOlderHistory(): Promise<void> {
    if (this._historyLoading || this._historyExhausted) return;
    if (!this.historyHandler || !this.state) return;
    // Scroll-back is a TRANSCRIPT affordance. A persona home or live face
    // also lives in `.what` (and a persona home OPENS at scrollTop 0, which
    // trips the near-top trigger) — never page chat history under those.
    if (focusedPersonaTab(this.nav) || focusedLiveTab(this.nav) || this.liveFace) return;
    const vm = chatViewModel(this.state);
    // No anchor = the live window is EMPTY (post-cursor reboot: only events
    // after the consumer watermark fold — the durable transcript still holds
    // everything). An anchor-less fetch pages the LATEST stored window, so a
    // freshly-rebooted room hydrates instead of sitting on "No messages yet".
    const anchor = this._history[0]?.id ?? vm.messages[0]?.id;
    this._historyLoading = true;
    const what = this.renderRoot.querySelector('.what');
    const prevHeight = what?.scrollHeight ?? 0;
    const prevTop = what?.scrollTop ?? 0;
    try {
      const page = await this.historyHandler(this.state.room_id, anchor);
      const onScreen = new Set([...this._history.map((r) => r.id), ...vm.messages.map((m) => m.id)]);
      const rows = historyRowsFromPoll(page, vm.members, onScreen);
      if (rows.length === 0) {
        this._historyExhausted = true;
        return;
      }
      this._history = [...rows, ...this._history];
      await this.updateComplete;
      // Keep the row the reader was looking at stationary: the prepend grew
      // the scrollable height above the viewport by exactly the delta.
      if (what) what.scrollTop = prevTop + (what.scrollHeight - prevHeight);
    } catch (err) {
      // Surface in the same strip as send failures — never a silently-dead scroll.
      this._selectError = `History load failed: ${err instanceof Error ? err.message : String(err)}`;
    } finally {
      this._historyLoading = false;
    }
  }

  /** The transcript's scroll listener: (1) near the top → page older history;
   *  (2) reader-intent tracking — a USER scroll away from the bottom parks
   *  auto-scroll, returning to the bottom re-arms it. Programmatic scrolls
   *  are guarded out so pin-to-bottom never reads as intent. */
  private onWhatScroll = (e: Event): void => {
    const el = e.currentTarget as Element;
    if (el.scrollTop < 120) void this.loadOlderHistory();
    if (this._autoScrolling) return;
    this._userScrolledUp = el.scrollTop + el.clientHeight < el.scrollHeight - 40;
  };

  /** Keep the compose input from scrolling on every state push. The persona
   *  home reads top-down (a profile, not a transcript) — never auto-scrolled,
   *  and OPENING one resets the pane to the top (the transcript underneath was
   *  pinned to its bottom; a profile that opens mid-scroll reads broken). */
  protected override updated(changed: PropertyValues): void {
    const persona = focusedPersonaTab(this.nav);
    // Scroll-back trigger: keep the transcript's scroll listener attached to
    // the CURRENT `.what` (Lit keeps the element stable across renders; the
    // identity check re-attaches if a face swap ever replaces it).
    const what = this.renderRoot.querySelector('.what');
    if (what && what !== this._scrollHost) {
      this._scrollHost?.removeEventListener('scroll', this.onWhatScroll);
      what.addEventListener('scroll', this.onWhatScroll, { passive: true });
      this._scrollHost = what;
    }
    // Pin to the live edge on new content (messages AND stream deltas)
    // unless the READER deliberately scrolled up — intent, not position.
    if ((changed.has('state') || changed.has('_typing')) && !persona && !this._userScrolledUp) {
      this._autoScrolling = true;
      this.scrollToLatest();
      // Release after the scroll's events flush (scroll events are sync-ish
      // but smooth-behavior can trail; a frame is enough for instant jumps).
      requestAnimationFrame(() => {
        this._autoScrolling = false;
      });
    }
    // Hydrate a sparse window from durable storage: after a cursor-armed core
    // reboot the live projection starts (near-)empty — page the latest stored
    // window in once so the room reads continuous, never "blown away". The
    // loading/exhausted latches stop this repeating.
    if (
      changed.has('state') &&
      !persona &&
      this.state !== undefined &&
      this._history.length === 0 &&
      chatViewModel(this.state).messages.length < 10
    ) {
      void this.loadOlderHistory();
    }
    if (changed.has('nav')) {
      const wasPersona = focusedPersonaTab(changed.get('nav') as NavViewState | undefined);
      if (persona && persona.id !== wasPersona?.id) {
        const what = this.renderRoot.querySelector('.what');
        if (what) what.scrollTop = 0;
      }
    }
    // Paint any live video frames onto their tile canvases (imperative — the
    // canvas element is declarative in the template but its pixels are not).
    if (this._videoFrames.size > 0) {
      for (const [sender, frame] of this._videoFrames) {
        const canvas = this.renderRoot.querySelector<HTMLCanvasElement>(
          `canvas.lt-video[data-sender="${sender}"]`,
        );
        if (!canvas || frame.pixelFormat !== 0) continue; // 0 = RGBA8
        canvas.width = frame.width;
        canvas.height = frame.height;
        const ctx = canvas.getContext('2d');
        if (!ctx) continue;
        const expected = frame.width * frame.height * 4;
        if (frame.pixels.length < expected) continue;
        // Copy into a fresh ArrayBuffer-backed clamped array (ImageData needs a
        // real ArrayBuffer, not a subarray view over the frame's buffer).
        const rgba = new Uint8ClampedArray(expected);
        rgba.set(frame.pixels.subarray(0, expected));
        ctx.putImageData(new ImageData(rgba, frame.width, frame.height), 0, 0);
      }
    }

    // Element navigation (card 95844639): the anchored persona home rendered —
    // land on its section (top-scroll above may have just run; the anchor wins).
    const pending = this._pendingAnchor;
    if (pending && persona?.id === pending.persona) {
      const section = this.renderRoot.querySelector(
        `.persona-home[data-persona="${pending.persona}"] #${pending.anchor}`,
      );
      if (section) {
        section.scrollIntoView({ block: 'start' });
        // Layout above the section (avatar image decode, meters) settles AFTER
        // this first scroll and pushes the card down (~400px observed live) —
        // re-land on the next two frames so the anchor actually sticks at the
        // pane top instead of drifting mid-pane.
        requestAnimationFrame(() => {
          section.scrollIntoView({ block: 'start' });
          requestAnimationFrame(() => section.scrollIntoView({ block: 'start' }));
        });
        this._pendingAnchor = null;
      }
    }
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
