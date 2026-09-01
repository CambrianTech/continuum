/**
 * The `live` activity's neutral `Content` body — a room's LIVE call face.
 *
 * A live call is NOT a special view: it is a room PURPOSE (`LIVE_PURPOSE`),
 * reached by the same nav semantics as any activity and rendered by a
 * purpose-registered `Content` renderer — exactly as `chat`/`foundry`/`persona`
 * dispatch theirs (ACTIVITY-ROOM-PATTERNS.md; tabs == rooms == activities,
 * [[room-purpose-is-per-recipe-not-an-enum]]). This file holds only the SHAPES —
 * consumer-neutral, DOM-free, ANSI-free — so web/tui/RAG all draw the same
 * projected body.
 *
 * Honesty contract ([[fallbacks-are-illegal-fail-loud]]): every field is REAL
 * today or explicitly absent/disabled:
 *   - a participant tile is the citizen's stored avatar + live presence — the
 *     browser media plane (real video tracks) is the documented follow-up
 *     (`mediaPlaneLive: false` until it lands; core-side LiveKit exists, the
 *     web pipeline does not);
 *   - `speaking` is the LIVE token rail (StreamDelta) — real tokens flowing
 *     from that citizen's in-progress turn RIGHT NOW, the same signal the
 *     roster tile's speaking ring draws;
 *   - the caption is that streaming turn's text — a real live transcript line;
 *   - a control renders enabled ONLY when its action is real (`hangup`,
 *     `captions`); mic/camera/screenshare are honestly disabled until the
 *     media plane lands. Never a fake toggle.
 */

/** The `Content` purpose key the live call face dispatches on — the live
 *  sibling of `'chat'` / `'foundry'` / `'persona'`. A room recipe that declares
 *  this purpose IS a live room; the client's Go-live affordance opens the same
 *  face for any room until such recipes exist. */
export const LIVE_PURPOSE = 'live';

/** One participant tile of the call grid. */
export interface LiveParticipantVM {
  /** The citizen's id (roster member id). */
  readonly id: string;
  /** Display name — the tile's name tag. */
  readonly name: string;
  /** Neutral member kind ('human' | 'agent' | 'system') — drives the glyph
   *  fallback when no avatar image exists. */
  readonly kind: string;
  /** Stored avatar image URL; absent = the tile draws the kind glyph —
   *  honest fallback, never a broken image. */
  readonly avatarUrl?: string;
  /** Live presence — attached and ready (the tile's status dot). */
  readonly active: boolean;
  /** SPEAKING NOW — this citizen's turn is streaming tokens on the live rail
   *  (StreamDelta) this instant. Drives the green speaking border. */
  readonly speaking: boolean;
  /** Self-reported runtime origin ("devstral", "claude", "" = unresolved). */
  readonly runtime: string;
  /** A live video frame is arriving for this participant (the media plane is
   *  painting real pixels onto the tile canvas). False = avatar/glyph only. */
  readonly hasVideo?: boolean;
  /** Video arrives as a LiveKit track (tile renders <video>, host attaches). */
  readonly lkVideo?: boolean;
}

/** The live caption line — the ACTIVE speaker's in-progress turn text (the
 *  real transcript of what is being "said" right now). */
export interface LiveCaptionVM {
  readonly speakerId: string;
  readonly speakerName: string;
  /** Tail of the streaming turn (already tail-clipped by the projection so the
   *  strip reads like rolling captions, not a growing wall). */
  readonly text: string;
}

/** The call-controls bar — availability is HONEST: a flag is true only when
 *  the action behind the button is real today. */
export interface LiveControlsVM {
  /** Microphone capture — TRUE when the browser CallClient is connected to
   *  the core's call server (the media plane is REAL for this reader). */
  readonly micAvailable: boolean;
  /** Mic currently capturing (the button's lit state). */
  readonly micOn?: boolean;
  /** Camera capture — TRUE when the CallClient is connected (humans are
   *  video participants inherently). */
  readonly cameraAvailable: boolean;
  /** Camera currently publishing (the button's lit state). */
  readonly cameraOn?: boolean;
  /** Screenshare — false until the browser media plane lands. */
  readonly screenshareAvailable: boolean;
  /** The caption strip toggle — real (it toggles the live transcript line). */
  readonly captionsAvailable: boolean;
  /** Whether captions are currently shown (renderer state, threaded through
   *  the projection so every target draws the toggle's true state). */
  readonly captionsOn: boolean;
  /** Hang-up — real (returns to the room's chat face via the nav seam). */
  readonly hangupAvailable: boolean;
  /** REAL durable transcript length of this room — the transcript badge's
   *  count (the full transcript panel is a follow-up; the count is live). */
  readonly transcriptCount: number;
}

/** The live call face's `Content` body (`purpose === LIVE_PURPOSE`). */
export interface LiveContentBody {
  /** The reader's pinned tile (explicit stage choice); absent = unpinned. */
  readonly pinnedId?: string;
  /** Senders with live LiveKit AUDIO tracks — one hidden <audio> each. */
  readonly lkAudioSenders?: ReadonlyArray<string>;
  /** The staged media-permission card (POSITRON-MEDIA-PERMISSIONS.md):
   *  present = the face shows the reason (or, when `denied`, recovery) card
   *  for that capability. Absent = no card. */
  readonly mediaAsk?: { readonly kind: 'camera' | 'mic'; readonly denied?: boolean };
  /** The room this call face belongs to. */
  readonly roomId: string;
  readonly roomName: string;
  /** The call grid, roster order (stable — tiles never jump when someone
   *  starts speaking; the border moves, not the tile). */
  readonly participants: readonly LiveParticipantVM[];
  /** The active speaker's live caption; absent = no one is streaming a turn
   *  right now, or captions are toggled off — the strip simply isn't drawn. */
  readonly caption?: LiveCaptionVM;
  readonly controls: LiveControlsVM;
  /** Whether real A/V tracks flow browser-side. FALSE today — tiles are the
   *  stored avatar + live presence/speaking; the LiveKit web pipeline is the
   *  documented follow-up. A renderer may surface this honestly ("avatar
   *  presence") — never fake a video frame. */
  readonly mediaPlaneLive: boolean;
}
