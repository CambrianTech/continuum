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

import { LitElement, html, nothing, type PropertyValues, type TemplateResult } from 'lit';
import type { ArenaViewState, CanvasViewState, ChatState } from '@continuum/chat-view';
import {
  chatViewModel,
  PROJECT_PURPOSE,
  focusedLiveTab,
  focusedPersonaTab,
  historyRowsFromPoll,
  type MessageRowVM,
  type RosterMemberVM,
} from '@continuum/chat-view';
import type {
  KanbanViewState,
  NavViewState,
  ServingViewState,
  StreamDelta,
  SystemMetricsViewState,
} from '@continuum/sdk-typescript';
import { renderChat } from './renderChat';
import { chatWidgetStyles } from './styles';
import { CallClient, type CallVideoFrame } from '../live/callClient';
import type { RemoteTrack } from 'livekit-client';
import {
  LISTING_SELECT,
  LIVE_CAMERA_TOGGLE,
  LIVE_MEDIA_ASK,
  LIVE_TILE_PIN,
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
import { universeRegistry, type UniverseInstance } from '../universe/Universe';
import { CosmosUniverse } from '../universe/CosmosUniverse';
import { PositronUniverse } from '../universe/PositronUniverse';

// One registration per universe — the registry is the only list (no switches;
// the widget resolves keys and speaks the UniverseInstance interface only).
universeRegistry.register(new CosmosUniverse());
universeRegistry.register(new PositronUniverse());

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
    board: { attribute: false },
    arena: { attribute: false },
    canvas: { attribute: false },
    version: { attribute: false },
    feedStatus: { attribute: false },
    mindRevision: { attribute: false },
    viewerId: { attribute: false },
    viewerName: { attribute: false },
    liveTokenHandler: { attribute: false },
    sendHandler: { attribute: false },
    settingsHandler: { attribute: false },
    selectRoomHandler: { attribute: false },
    liveFace: { attribute: false },
    // Deep-link intents are set by index.ts BEFORE the element upgrades; only a
    // declared property survives Lit's upgrade (a plain class field's initializer
    // clobbers a pre-upgrade set — the intent silently never fired, 2026-09-05).
    autoMic: { attribute: false },
    autoCam: { attribute: false },
    micMode: { attribute: false },
    liveRoom: { attribute: false },
    callUrl: { attribute: false },
    _mediaConnected: { state: true },
    _micOn: { state: true },
    _camOn: { state: true },
    _mediaAsk: { state: true },
    _pinnedTile: { state: true },
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

  /** The state feed's connection status (`live`/`connecting`/`reconnecting`/
   *  `cached`/`closed`), set by the host's `StateConnection.onStatus` — drives
   *  the continuon orb's color (with the favicon, the designed status channel;
   *  never a text banner). */
  feedStatus?: string;

  /** Monotonic bump from the host's mind-feed poller (renderPersona's live
   *  learning stream reads a module store; this property exists purely so a
   *  fresh poll re-renders the open persona page). */
  mindRevision = 0;

  /** The VIEWER's durable identity (the session's `?me=` uuid) — who this
   *  human IS in a live call. Set by the host; joining a call without it is
   *  refused loudly (the hardcoded 'operator-web' string it replaces made the
   *  registrar drop the session — hold music, no citizens; live 2026-08-31). */
  viewerId?: string;
  /** The viewer's display name for call tiles (the directory's name for
   *  `viewerId`, e.g. "joel"). */
  viewerName?: string;

  /** Host-supplied `live/token` fetch — present = the node has the WebRTC
   *  media plane and calls join it (UDP, hardware codecs). Absent = the WS
   *  tee remains the only lane (older core), loudly logged by the client. */
  liveTokenHandler?: (room: string) => Promise<{ url: string; token: string }>;

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
  /** Deep-link intents (`?mic`, `?cam`): publish as soon as the call connects. */
  autoMic = false;
  autoCam = false;
  /** Which capture path `?mic=` asked for (self-exercise isolation); 'auto' for humans. */
  micMode: 'auto' | 'lk' | 'ws' = 'auto';
  private _intentBusy = false;
  private _intentTries = 0;
  /** The room a `?live` deep link named — the call connects only once the
   *  focused state IS that room (by name or id), never the daemon's prior focus. */
  liveRoom?: string;

  /** ws:// URL of the core's call server (config: CONTINUUM_CALL_WS, default
   *  8790). Absent = no media plane; the live face stays avatar-presence with
   *  the mic honestly disabled. */
  callUrl?: string;

  private _call?: CallClient;
  private _mediaConnected = false;
  private _micOn = false;
  private _camOn = false;
  /** The staged media-permission flow (POSITRON-MEDIA-PERMISSIONS.md): which
   *  capability's reason/recovery card the live face is showing. `denied`
   *  switches the card from reason+continue to honest recovery steps. */
  private _mediaAsk?: { kind: 'camera' | 'mic'; denied?: boolean };
  /** Call-server avatar states: personaId → speaking (merged into the streams
   *  map so the SAME projection drives borders whether speech is tokens on the
   *  chat rail or real audio on the call). */
  private _callSpeaking = new Map<string, boolean>();
  /** senderId → latest decoded video frame, painted onto the tile canvas in
   *  updated() (imperative — canvas is not declarative Lit content). */
  private _videoFrames = new Map<string, CallVideoFrame>();
  /** LiveKit tracks by participant identity — the REAL media plane. Attached
   *  imperatively after render (video → its tile's <video>, audio → a hidden
   *  autoplaying <audio>); the browser decodes off-main-thread. */
  private _lkTracks = new Map<string, { video?: RemoteTrack; audio?: RemoteTrack }>();
  /** Who is actually IN the call (ws ParticipantJoined + lk track holders +
   *  self) — the grid tiles THESE, not everyone room-online (a call app never
   *  shows offline/absent people as static tiles; they appear on connect). */
  private _callParticipants = new Set<string>();
  /** The reader's pinned call tile (explicit stage); undefined = unpinned. */
  private _pinnedTile?: string;
  /** Pending typing mutations (text = latest accumulation, null = ended) —
   *  flushed onto the reactive `_typing` at most once per animation frame. */
  private _typingBuf = new Map<string, string | null>();
  private _typingFlush?: number;

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
      onParticipantJoined: (userId) => {
        this._callParticipants = new Set(this._callParticipants).add(userId);
        this.requestUpdate();
      },
      onParticipantLeft: (userId) => {
        const next = new Set(this._callParticipants);
        next.delete(userId);
        this._callParticipants = next;
        this.requestUpdate();
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
      onTrack: (identity, kind, track) => {
        const entry = this._lkTracks.get(identity) ?? {};
        entry[kind] = track;
        if (entry.video === undefined && entry.audio === undefined) {
          this._lkTracks.delete(identity);
        } else {
          this._lkTracks.set(identity, entry);
        }
        // Structural: a tile's media element set changed — one re-render, then
        // the post-render pass attaches the tracks imperatively.
        this.requestUpdate();
      },
      onVideoFrame: (f) => {
        // OVERDRAW LAW (Joel, live 2026-08-31): a video frame repaints ITS
        // canvas, never the widget. requestUpdate here re-rendered the entire
        // template per frame per sender — the browser burned 40% GPU on
        // overdraw. Structural re-render happens ONLY when a sender first
        // appears (its tile/canvas doesn't exist yet).
        const isNew = !this._videoFrames.has(f.senderId);
        this._videoFrames.set(f.senderId, f);
        if (isNew) {
          this.requestUpdate();
          return; // the post-render paint pass below draws the first frame
        }
        this.paintVideoFrame(f);
      },
    });
    this._call = client;
    if (this.viewerId === undefined) {
      // No fallback identity ([[fallbacks-are-illegal-fail-loud]]): a call
      // joined under a made-up id is a session the registrar cannot address —
      // the exact hold-music bug this replaces.
      console.error('cannot join call: viewerId not set on <chat-widget> — the host must set it');
      this._call = undefined;
      this._mediaConnected = false;
      return;
    }
    // The media-plane door: live/token from the host. A missing handler or a
    // failed mint is LOUD and the call proceeds control-only (WS) — never a
    // silent fake-connected media state.
    let livekit: { url: string; token: string } | undefined;
    if (this.liveTokenHandler !== undefined) {
      try {
        livekit = await this.liveTokenHandler(this.state.room_id);
      } catch (err) {
        console.error('live/token failed — media plane unavailable this call:', err);
      }
    }
    try {
      await client.connect(
        this.callUrl,
        this.state.room_id,
        this.viewerId,
        this.viewerName ?? 'you',
        livekit,
      );
    } catch {
      this._call = undefined;
      this._mediaConnected = false;
    }
  }

  /** Paint ONE video frame onto its tile canvas — imperative, render-pass-free. */
  private paintVideoFrame(frame: CallVideoFrame): void {
    if (frame.pixelFormat !== 0) return; // 0 = RGBA8
    const canvas = this.renderRoot.querySelector<HTMLCanvasElement>(
      `canvas.lt-video[data-sender="${frame.senderId}"]`,
    );
    if (!canvas) return;
    if (canvas.width !== frame.width) canvas.width = frame.width;
    if (canvas.height !== frame.height) canvas.height = frame.height;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    const expected = frame.width * frame.height * 4;
    if (frame.pixels.length < expected) return;
    const rgba = new Uint8ClampedArray(expected);
    rgba.set(frame.pixels.subarray(0, expected));
    ctx.putImageData(new ImageData(rgba, frame.width, frame.height), 0, 0);
  }

  private disconnectCall(): void {
    this._call?.leave();
    this._call = undefined;
    this._mediaConnected = false;
    this._micOn = false;
    this._camOn = false;
    this._callSpeaking = new Map();
    this._videoFrames = new Map();
    this._lkTracks = new Map();
    this._callParticipants = new Set();
    this._pinnedTile = undefined;
  }

  /** The mic button — a REAL toggle when the media plane is connected. */
  /** The platform's LIVE permission state for a capability — queried, never
   *  assumed (the OS/browser is the single authority,
   *  POSITRON-MEDIA-PERMISSIONS.md). Safari lacks the Permissions API:
   *  report 'prompt', which stages the reason card first — stage-1 behavior
   *  regardless. */
  private async mediaPermission(kind: 'camera' | 'mic'): Promise<'granted' | 'prompt' | 'denied'> {
    try {
      const status = await navigator.permissions.query({
        name: (kind === 'camera' ? 'camera' : 'microphone') as PermissionName,
      });
      return status.state as 'granted' | 'prompt' | 'denied';
    } catch {
      return 'prompt';
    }
  }

  /** Stage 2: the ASK, fired from an explicit gesture (the reason card's
   *  continue button, or the capability button once already granted). */
  private startMedia(kind: 'camera' | 'mic'): void {
    const call = this._call;
    if (!call) return;
    this._mediaAsk = undefined;
    if (kind === 'camera') {
      void call.startCamera(this.viewerId).then((ok) => {
        this._camOn = ok;
        if (!ok) this._mediaAsk = { kind: 'camera', denied: true };
      });
    } else {
      void call.startMic(this.micMode).then((ok) => {
        this._micOn = ok;
        if (!ok) this._mediaAsk = { kind: 'mic', denied: true };
      });
    }
  }

  /** A capability button press → the staged flow: granted acts directly;
   *  prompt opens the reason card; denied opens recovery. */
  private toggleMedia(kind: 'camera' | 'mic'): void {
    const call = this._call;
    if (!call) return;
    if (kind === 'camera' && call.camLive) {
      call.stopCamera();
      this._camOn = false;
      return;
    }
    if (kind === 'mic' && call.micLive) {
      call.stopMic();
      this._micOn = false;
      return;
    }
    void this.mediaPermission(kind).then((state) => {
      if (state === 'granted') this.startMedia(kind);
      else this._mediaAsk = { kind, denied: state === 'denied' };
    });
  }

  /** Escape unpins the stage (legacy LiveWidget behaviour): the grid is the
   *  resting state; a pin is a gesture the same key undoes. */
  private onLiveKeydown = (e: KeyboardEvent): void => {
    if (e.key === 'Escape' && this._pinnedTile !== undefined) this._pinnedTile = undefined;
  };

  /** Tile click: pin it to the stage; clicking the pinned tile unpins. */
  private onLiveTilePin = (e: Event): void => {
    const id = (e as CustomEvent<{ id?: string }>).detail?.id;
    if (id === undefined) return;
    this._pinnedTile = this._pinnedTile === id ? undefined : id;
  };

  /** The reason card's outcome: continue → the real ask on this explicit
   *  gesture; dismiss (and the denied card's OK) → close. */
  private onLiveMediaAsk = (e: Event): void => {
    const outcome = (e as CustomEvent<{ outcome?: string }>).detail?.outcome;
    if (outcome === 'continue' && this._mediaAsk && !this._mediaAsk.denied) {
      this.startMedia(this._mediaAsk.kind);
    } else {
      this._mediaAsk = undefined;
    }
  };

  /** The camera button — the staged permission flow, then start/stop. */
  private onLiveCameraToggle = (): void => {
    this.toggleMedia('camera');
  };

  private onLiveMicToggle = (): void => {
    this.toggleMedia('mic');
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
    // COALESCED: tokens arrive faster than frames, and assigning the reactive
    // `_typing` per token re-rendered the full template per token (the
    // overdraw class, Joel live 2026-08-31). Mutations buffer into a pending
    // op map (string = latest text, null = stream ended) and the ONE reactive
    // assignment flushes at most once per animation frame.
    if (delta.done) {
      this._typingBuf.set(delta.senderId, null);
    } else {
      const base =
        this._typingBuf.get(delta.senderId) ?? this._typing.get(delta.senderId) ?? '';
      this._typingBuf.set(delta.senderId, (base ?? '') + delta.token);
    }
    if (this._typingFlush === undefined) {
      this._typingFlush = requestAnimationFrame(() => {
        this._typingFlush = undefined;
        const next = new Map(this._typing);
        for (const [k, v] of this._typingBuf) {
          if (v === null) next.delete(k);
          else next.set(k, v);
        }
        this._typingBuf.clear();
        this._typing = next;
      });
    }
  }

  override connectedCallback(): void {
    super.connectedCallback();
    // The UNIVERSE axis: ?universe=<key> re-embodies the SAME chatApp inside a whole
    // experience — Tron's neon grid, a fantasy forge, a corporate onboarding — theme +
    // lore, one definition, every citizen ([[universe-is-an-experience-not-a-theme]]).
    // The target maps the key to a skin; here it's a data-attribute the styles key off.
    // Unset → the native 'continuum' look.
    // Universes are OPT-IN (?universe=<key>) and resolved through the
    // registry — a universe earns default status only after passing the
    // paint budget (the 2026-08-31 cosmos crash wrote that law).
    const universe = new URLSearchParams(location.search).get('universe');
    if (universe && universe !== 'none') {
      this.setAttribute('data-universe', universe);
      const world = universeRegistry.get(universe);
      if (world) this._universe = world.mount();
      else console.warn(`unknown universe key '${universe}' — no such registration`);
    }
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
    this.addEventListener(LIVE_CAMERA_TOGGLE, this.onLiveCameraToggle);
    this.addEventListener(LIVE_MEDIA_ASK, this.onLiveMediaAsk);
    this.addEventListener(LIVE_TILE_PIN, this.onLiveTilePin);
    this.addEventListener('keydown', this.onLiveKeydown);
    this.addEventListener(LIVE_CAPTIONS_TOGGLE, this.onLiveCaptionsToggle);
  }

  override disconnectedCallback(): void {
    this._universe?.dispose();
    this._universe = undefined;
    this.removeEventListener(MESSAGE_EXPAND_TOGGLE, this.onExpandToggle);
    this.removeEventListener(LISTING_SELECT, this.onListingSelect);
    this.removeEventListener(NAV_TAB_CLOSE, this.onNavTabClose);
    this.removeEventListener(LIVE_FACE_TOGGLE, this.onLiveFaceToggle);
    this.removeEventListener(LIVE_CAPTIONS_TOGGLE, this.onLiveCaptionsToggle);
    this.removeEventListener(LIVE_TILE_PIN, this.onLiveTilePin);
    this.removeEventListener('keydown', this.onLiveKeydown);
    super.disconnectedCallback();
  }

  static override styles = chatWidgetStyles;

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
      // The directory's heartbeat recency beats a room card's stale stamp —
      // the "9d ago" rows were identity-creation dates, not activity.
      const seedSeen = new Map(this.directorySeed.map((m) => [m.id, m.lastSeenMs]));
      // Faces ride the directory too: the presence stream's avatar enrichment
      // only lands on changed-roster publishes (heartbeats are channel-scoped,
      // so fossil rosters never re-publish) — the seed's portrait fills any
      // member the stream left faceless.
      const seedFace = new Map(
        this.directorySeed.flatMap((m) => (m.avatarUrl ? [[m.id, m.avatarUrl] as const] : [])),
      );
      vm = {
        ...vm,
        members: vm.members.map((m) => {
          const fresher = Math.max(m.lastSeenMs, seedSeen.get(m.id) ?? 0);
          const active = m.active || resident.has(m.id);
          const face = m.avatarUrl ?? seedFace.get(m.id);
          return active !== m.active || fresher !== m.lastSeenMs || face !== m.avatarUrl
            ? { ...m, active, lastSeenMs: fresher, ...(face !== undefined ? { avatarUrl: face } : {}) }
            : m;
        }),
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
      // STABLE order: online first, then NAME. lastSeenMs is deliberately NOT
      // a sort key — it refreshes on every directory poll, and a volatile sort
      // key made the whole rail (and the call grid downstream) visibly
      // reshuffle on every render tick (Joel, live 2026-08-31: "all the boxes
      // randomly reordered"). Recency still renders as the row's stamp; it
      // just doesn't move rows around.
      const merged = [...vm.members, ...offRoom].sort(
        (a, b) => Number(b.active) - Number(a.active) || a.name.localeCompare(b.name),
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
        : html`<div class="responding-line wave-active">(${responders.join(', ')}) is responding…</div>`;
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
        directory: [...this._directory.values()],
        sys: this.sys,
        serving: this.serving,
        board: this.board,
        arena: this.arena,
        canvas: this.canvas,
        version: this.version,
        ...(this.feedStatus ? { feed: this.feedStatus } : {}),
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
          cameraOn: this._camOn,
          ...(this._mediaAsk !== undefined ? { mediaAsk: this._mediaAsk } : {}),
          ...(this._pinnedTile !== undefined ? { pinnedId: this._pinnedTile } : {}),
          videoSenders: Array.from(this._videoFrames.keys()),
          lkVideoSenders: [...this._lkTracks.entries()]
            .filter(([, t]) => t.video !== undefined)
            .map(([id]) => id),
          lkAudioSenders: [...this._lkTracks.entries()]
            .filter(([, t]) => t.audio !== undefined)
            .map(([id]) => id),
          callParticipants: [
            ...new Set([
              ...(this.viewerId !== undefined ? [this.viewerId] : []),
              ...this._callParticipants,
              ...this._lkTracks.keys(),
            ]),
          ],
        },
      }, { centerFooter });
    } catch (err) {
      const cause = err instanceof Error ? err.message : String(err);
      return html`<div class="render-error">Interface error rendering this room: ${cause}</div>`;
    }
    if (this._universe) {
      this._universe.update({
        citizens: vm.members.map((m) => ({ name: m.name, active: m.active })),
      });
    }
    return html`
      ${this._universe?.element ?? nothing}
      ${surface}
    `;
  }

  /** Derive history-buffer state BEFORE render (the Lit hook for exactly
   *  this): a room switch clears the buffer; while the buffer is open, live
   *  rows that slid out of the 50-row window RETIRE onto the buffer's tail —
   *  otherwise a new message would open a silent gap between scrolled-back
   *  history and the live window. */
  private _lastMsgCount = -1;
  private _lastActCount = -1;
  /** The mounted universe, spoken to ONLY through UniverseInstance. */
  private _universe?: UniverseInstance;

  protected override willUpdate(changed: PropertyValues): void {
    // Universe facts — one interface call per REAL event delta, whatever
    // world is mounted (or none). No element spelunking, ever.
    if (changed.has('state') && this.state && this._universe) {
      const msgs = this.state.messages.length;
      const acts = this.state.acts.length;
      if (this._lastMsgCount >= 0 && msgs > this._lastMsgCount) this._universe.onFact({ kind: 'message' });
      if (this._lastActCount >= 0 && acts > this._lastActCount) this._universe.onFact({ kind: 'act' });
      this._lastMsgCount = msgs;
      this._lastActCount = acts;
    }
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
    // A live face that is OPEN and has no call yet connects one — whether it was
    // opened by the Go-live toggle or by a `?live` deep link at boot (the toggle
    // handler was the only caller of connectCall, so a deep link — and the
    // node's own self-test through the eye-node — showed the face with no
    // session, no media plane, every live probe at zero; 2026-09-05).
    const routedRoomIsFocused =
      this.liveRoom === undefined ||
      this.state?.room_name === this.liveRoom ||
      this.state?.room_id === this.liveRoom;
    if (this.liveFace && !this._call && this.state && this.callUrl && routedRoomIsFocused) void this.connectCall();
    // Deep-link media intents fire once the call is connected and only while
    // that media is off — idempotent across renders (startMedia flips _micOn/_camOn).
    // An INTENT retries until the media plane can carry it (LiveKit connects
    // after the core socket, and a LiveKit-only start returns false until then);
    // it never opens the permission card and never marks "denied" — a human's
    // click still goes through startMedia/mediaPermission. Bounded: 40 renders.
    // GLASS BOX for the eye: the intent's state as a host attribute, so a
    // structure walk (perception/observe) reads it without a console.
    if (this.liveFace) {
      this.setAttribute(
        'data-live-intent',
        `mic=${this.autoMic ? this.micMode : 'off'} cam=${this.autoCam} call=${this._call !== undefined} conn=${this._mediaConnected} micOn=${this._micOn} tries=${this._intentTries} busy=${this._intentBusy} lk=${this._call?.lkLive}`,
      );
    }
    if (this.liveFace && this._call && this._mediaConnected && (this.autoMic || this.autoCam)) {
      const call = this._call;
      if (this.autoMic && !this._micOn && !this._intentBusy && this._intentTries < 40) {
        this._intentBusy = true;
        this._intentTries += 1;
        void call.startMic(this.micMode).then((ok) => {
          console.warn(`[live-intent] mic try ${this._intentTries} mode=${this.micMode} ok=${ok}`);
          this._intentBusy = false;
          if (ok) {
            this._micOn = true;
            this.autoMic = false;
          } else {
            setTimeout(() => this.requestUpdate(), 500); // the plane may be live on the next render
          }
        });
      }
      if (this.autoCam && !this._camOn && !this._intentBusy && this._intentTries < 40) {
        this._intentBusy = true;
        this._intentTries += 1;
        void call.startCamera(this.viewerId).then((ok) => {
          this._intentBusy = false;
          if (ok) {
            this._camOn = true;
            this.autoCam = false;
          } else {
            setTimeout(() => this.requestUpdate(), 500);
          }
        });
      }
    }
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
    if ((changed.has('state') || changed.has('_typing')) && !persona && this.state?.purpose !== PROJECT_PURPOSE && !this._userScrolledUp) {
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
      // First-paint only: steady-state frames paint imperatively in
      // onVideoFrame without touching the render pass.
      for (const frame of this._videoFrames.values()) this.paintVideoFrame(frame);
    }
    // Attach LiveKit tracks to their media elements (idempotent — attach() on
    // an already-attached element is a no-op srcObject set). The browser owns
    // decode + paint from here; zero per-frame JS.
    for (const [identity, tracks] of this._lkTracks) {
      if (tracks.video !== undefined) {
        const el = this.renderRoot.querySelector<HTMLVideoElement>(
          `video[data-lk-video="${identity}"]`,
        );
        if (el && el.srcObject !== tracks.video.mediaStream) tracks.video.attach(el);
      }
      if (tracks.audio !== undefined) {
        const el = this.renderRoot.querySelector<HTMLAudioElement>(
          `audio[data-lk-audio="${identity}"]`,
        );
        if (el && el.srcObject !== tracks.audio.mediaStream) tracks.audio.attach(el);
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
