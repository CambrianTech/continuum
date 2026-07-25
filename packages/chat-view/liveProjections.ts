/**
 * Live call face → pattern-primitive projections.
 *
 * A room's LIVE face is an activity like any other — dispatched by purpose
 * (`LIVE_PURPOSE`) through the one Content registry, exactly as the persona
 * home dispatches `PERSONA_PURPOSE` (ACTIVITY-ROOM-PATTERNS.md; never a
 * parallel route). This file is the PURE projection from state the surface
 * already holds onto the neutral `LiveContentBody` every target draws:
 *   - the roster snapshot (presence, avatars, kinds) → the participant grid;
 *   - the live StreamDelta token rail (senderId → accumulated in-progress
 *     text, the SAME widget-owned overlay that lights the roster's speaking
 *     ring) → per-tile `speaking` + the active speaker's caption line.
 *
 * Honesty ([[fallbacks-are-illegal-fail-loud]]): `speaking` is true only while
 * REAL tokens flow; the caption IS the streaming turn's text (a real live
 * transcript), tail-clipped so the strip reads like rolling captions; controls
 * advertise available only what is real (hang-up, captions) — mic/camera/
 * screenshare stay honestly disabled until the browser media plane lands
 * (core-side LiveKit exists; the web pipeline is the documented follow-up).
 */

import { LIVE_PURPOSE } from '@continuum/patterns';
import type {
  LiveCaptionVM,
  LiveContentBody,
  LiveControlsVM,
  LiveParticipantVM,
} from '@continuum/patterns';
import type { NavViewState } from '@continuum/sdk-typescript';
import type { ChatViewModel, RosterMemberVM } from './chatViewModel';

/** The widget-owned live-call overlay the host threads into the workspace
 *  projection — renderer state + the ephemeral token rail, NEVER substrate
 *  state (the same overlay discipline as `MessageRowVM.expanded` and the
 *  roster's speaking ring). */
export interface LiveCallOverlay {
  /** The reader opened this room's live face (the Go-live affordance). A
   *  focused live-purpose tab or a live-purpose room opens it too — see
   *  `liveFaceOpen`. */
  readonly open: boolean;
  /** senderId → accumulated in-progress turn text — the live StreamDelta rail
   *  (#170), who is "speaking" RIGHT NOW. Empty = silence. */
  readonly streams: Readonly<Record<string, string>>;
  /** Whether the reader has the caption strip on (renderer state). */
  readonly captionsOn: boolean;
  /** The browser MEDIA PLANE is connected (the CallClient joined the core's
   *  call server) — flips the honest capability flags below. Absent/false =
   *  avatar-presence face, exactly as before. */
  readonly mediaConnected?: boolean;
  /** Mic is currently capturing/publishing (renderer state via CallClient). */
  readonly micOn?: boolean;
}

/** The live-purpose tab currently focused in the citizen's nav view, if any —
 *  the recipe-driven entry: a room whose recipe declares purpose "live"
 *  carries it on its nav tab ([[room-purpose-is-per-recipe-not-an-enum]]; the
 *  core's room_purpose seam fills `NavTab.purpose`). The live sibling of
 *  `focusedPersonaTab`. */
export function focusedLiveTab(
  nav: NavViewState | undefined,
): { id: string; title: string } | undefined {
  if (!nav?.current_tab) return undefined;
  const tab = nav.open_tabs.find((t) => t.id === nav.current_tab && t.purpose === LIVE_PURPOSE);
  return tab ? { id: tab.id, title: tab.title } : undefined;
}

/** Whether the live face should render — ANY of the three honest entries:
 *  the room's own recipe purpose is "live", the focused nav tab is a
 *  live-purpose activity, or the reader opened the face via the Go-live
 *  affordance (the client entry until live recipes exist). */
export function liveFaceOpen(
  vm: ChatViewModel,
  nav: NavViewState | undefined,
  overlay: LiveCallOverlay | undefined,
): boolean {
  return (
    vm.purpose === LIVE_PURPOSE || focusedLiveTab(nav) !== undefined || overlay?.open === true
  );
}

/** How much streaming text the caption strip carries — the TAIL, so it reads
 *  like rolling captions (newest words visible), never a growing wall. */
export const CAPTION_TAIL_CHARS = 220;

/** Tail-clip one streaming turn into a caption line: whitespace collapsed
 *  (tokens arrive with newlines/runs), last `max` chars kept, an ellipsis
 *  marking the clipped head. Pure + deterministic (testable). */
export function captionTail(text: string, max = CAPTION_TAIL_CHARS): string {
  const flat = text.replace(/\s+/g, ' ').trim();
  if (flat.length <= max) return flat;
  return `…${flat.slice(flat.length - max)}`;
}

/** One roster member → a call-grid tile. `speaking` comes from the live rail:
 *  an entry in `streams` means tokens are flowing from this citizen NOW. */
function participant(
  m: RosterMemberVM,
  streams: Readonly<Record<string, string>>,
): LiveParticipantVM {
  return {
    id: m.id,
    name: m.name,
    kind: m.kind,
    ...(m.avatarUrl ? { avatarUrl: m.avatarUrl } : {}),
    active: m.active,
    speaking: Object.prototype.hasOwnProperty.call(streams, m.id),
    runtime: m.runtime,
  };
}

/** The call grid — the whole roster in roster order (stable tiles: the
 *  speaking border moves, the grid never reshuffles mid-call). */
export function liveParticipants(
  vm: ChatViewModel,
  streams: Readonly<Record<string, string>>,
): readonly LiveParticipantVM[] {
  return vm.members.map((m) => participant(m, streams));
}

/** The ACTIVE speaker's caption — the most recent citizen to start a turn
 *  (last entry of the insertion-ordered rail) whose identity the roster
 *  resolves and whose stream has text. `undefined` = silence (no strip drawn,
 *  never a fabricated line). */
export function liveCaption(
  vm: ChatViewModel,
  streams: Readonly<Record<string, string>>,
): LiveCaptionVM | undefined {
  const entries = Object.entries(streams);
  for (let i = entries.length - 1; i >= 0; i--) {
    const entry = entries[i];
    if (!entry) continue;
    const [senderId, text] = entry;
    if (!text.trim()) continue;
    const member = vm.members.find((m) => m.id === senderId);
    if (!member) continue; // unknown sender — never fabricate an identity
    return { speakerId: senderId, speakerName: member.name, text: captionTail(text) };
  }
  return undefined;
}

/** The controls bar's honest availability: hang-up + captions are real;
 *  mic/camera/screenshare are disabled until the browser media plane lands.
 *  The transcript badge count is the room's REAL durable transcript length. */
export function liveControls(
  vm: ChatViewModel,
  captionsOn: boolean,
  overlay?: LiveCallOverlay,
): LiveControlsVM {
  return {
    micAvailable: overlay?.mediaConnected === true,
    micOn: overlay?.micOn === true,
    cameraAvailable: false,
    screenshareAvailable: false,
    captionsAvailable: true,
    captionsOn,
    hangupAvailable: true,
    transcriptCount: vm.messages.length,
  };
}

/** Project the room's live call face from state the surface already holds.
 *  `mediaPlaneLive` is FALSE until real A/V tracks flow browser-side — the
 *  tiles are stored avatars + live presence/speaking, honestly labelled. */
export function liveContentBody(vm: ChatViewModel, overlay?: LiveCallOverlay): LiveContentBody {
  const streams = overlay?.streams ?? {};
  const captionsOn = overlay?.captionsOn ?? true;
  const caption = captionsOn ? liveCaption(vm, streams) : undefined;
  return {
    roomId: vm.roomId,
    roomName: vm.roomName,
    participants: liveParticipants(vm, streams),
    ...(caption ? { caption } : {}),
    controls: liveControls(vm, captionsOn, overlay),
    mediaPlaneLive: overlay?.mediaConnected === true,
  };
}
