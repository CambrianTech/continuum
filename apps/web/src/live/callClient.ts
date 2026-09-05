/**
 * `CallClient` — the browser side of the live-call MEDIA PLANE.
 *
 * Dials the core's native WebSocket call server (`live/transport/call_server.rs`
 * — mix-minus audio, STT transcription fan-out, avatar-state updates, video
 * frames) and turns the live face's honest placeholders into the real thing:
 * the mic button captures, remote citizens are HEARD, the speaking borders
 * follow the server's avatar states, and captions carry real STT text.
 *
 * Wire protocol (typed on the Rust side, ts-rs exported as CallMessage):
 *  - JSON text frames: Join/Leave/Mute … ParticipantJoined/Left, Transcription,
 *    AvatarUpdate, Stats, Error.
 *  - Binary frames, first byte = FrameKind:
 *      client→server audio: [0x01][PCM16 i16 LE @16kHz mono]
 *      server→client audio: [0x01][senderLen:u8][senderUserId][PCM16 i16 LE]
 *      server→client video: [0x02][senderLen:u8][senderUserId][header 16B][pixels]
 *
 * AUDIO IS OFF THE MAIN THREAD (CLAUDE.md non-negotiable): capture runs in an
 * AudioWorklet that downsamples to 16kHz Int16 and transfers buffers; playback
 * runs in a worklet fed per-sender sample queues. No ScriptProcessorNode.
 */

import type { StreamDelta } from '@continuum/sdk-typescript';
import {
  Room,
  RoomEvent,
  Track,
  createLocalAudioTrack,
  createLocalVideoTrack,
  type LocalAudioTrack,
  type LocalVideoTrack,
  type RemoteTrack,
} from 'livekit-client';

/** One remote participant's live media state, as the face consumes it. */
export interface CallAvatarState {
  readonly personaId: string;
  readonly speaking: boolean;
  readonly listening: boolean;
  readonly emotion: string;
  readonly viseme: number;
  readonly visemeWeight: number;
}

/** A decoded remote video frame (rgba8 path — the Bevy avatar feed). */
export interface CallVideoFrame {
  readonly senderId: string;
  readonly width: number;
  readonly height: number;
  readonly pixelFormat: number;
  readonly sequence: number;
  readonly pixels: Uint8Array;
}

export interface CallClientEvents {
  onConnected?: () => void;
  onClosed?: (reason: string) => void;
  onParticipantJoined?: (userId: string, displayName: string) => void;
  onParticipantLeft?: (userId: string) => void;
  onAvatar?: (state: CallAvatarState) => void;
  onTranscription?: (userId: string, text: string) => void;
  onVideoFrame?: (frame: CallVideoFrame) => void;
  /** A LiveKit media track (video or audio) for a participant appeared or
   *  vanished (`track === undefined`). The REAL plane: UDP, hardware codecs —
   *  the host attaches video tracks to <video> elements and audio tracks to
   *  autoplaying <audio>; no JS ever touches pixels. */
  onTrack?: (identity: string, kind: 'video' | 'audio', track?: RemoteTrack) => void;
  /** LiveKit's audio-level active-speaker set changed (the REAL "speaking",
   *  from audio energy — never the token rail). Empty when the room is quiet. */
  onActiveSpeakers?: (identities: ReadonlySet<string>) => void;
  /** Mirrors transcriptions onto the SAME StreamDelta shape the typing rail
   *  uses, so the existing caption/speaking plumbing needs zero new paths. */
  onDelta?: (delta: StreamDelta) => void;
}

const FRAME_AUDIO = 0x01;
const FRAME_VIDEO = 0x02;
const WIRE_SAMPLE_RATE = 16000;

/** Inline worklet code (Blob URL — the vite asset pipeline stays untouched and
 *  the CSP story stays same-origin). CAPTURE: mic float32 @ ctx rate →
 *  downsample to 16k Int16 → transfer. PLAYBACK: Int16 chunks → float ring →
 *  output. Both tiny, allocation-light, per the worklet contract. */
const WORKLET_SOURCE = `
class PcmCapture extends AudioWorkletProcessor {
  constructor() { super(); this._acc = []; this._accLen = 0; }
  process(inputs) {
    const ch = inputs[0] && inputs[0][0];
    if (!ch) return true;
    const ratio = sampleRate / ${WIRE_SAMPLE_RATE};
    const outLen = Math.floor(ch.length / ratio);
    const out = new Int16Array(outLen);
    for (let i = 0; i < outLen; i++) {
      const s = ch[Math.floor(i * ratio)];
      out[i] = Math.max(-32768, Math.min(32767, (s * 32767) | 0));
    }
    this.port.postMessage(out.buffer, [out.buffer]);
    return true;
  }
}
class PcmPlayer extends AudioWorkletProcessor {
  constructor() {
    super();
    this._queue = [];
    this._offset = 0;
    this.port.onmessage = (e) => { this._queue.push(new Int16Array(e.data)); };
  }
  process(_inputs, outputs) {
    const out = outputs[0][0];
    if (!out) return true;
    const ratio = ${WIRE_SAMPLE_RATE} / sampleRate;
    for (let i = 0; i < out.length; i++) {
      const src = this._queue[0];
      if (!src) { out[i] = 0; continue; }
      const idx = Math.floor(this._offset);
      out[i] = (src[idx] ?? 0) / 32768;
      this._offset += ratio;
      if (this._offset >= src.length) { this._queue.shift(); this._offset = 0; }
    }
    return true;
  }
}
registerProcessor('pcm-capture', PcmCapture);
registerProcessor('pcm-player', PcmPlayer);
`;

export class CallClient {
  private ws?: WebSocket;
  private ctx?: AudioContext;
  private player?: AudioWorkletNode;
  private captureNode?: AudioWorkletNode;
  private micStream?: MediaStream;
  private events: CallClientEvents;
  private closedByUs = false;

  /** True once Join is sent on an open socket. */
  connected = false;
  /** The REAL media plane (LiveKit/WebRTC over UDP). Connected when the host
   *  supplied `live/token` creds; the WS lane stays for CONTROL (join,
   *  transcriptions, avatar state) + the glass-box video tee. */
  private lkRoom?: Room;
  private lkMic?: LocalAudioTrack;
  private lkCam?: LocalVideoTrack;
  /** True when the WebRTC media plane is up — capture publishes there. */
  lkLive = false;
  /** True while the mic worklet is publishing. */
  micLive = false;

  constructor(events: CallClientEvents) {
    this.events = events;
  }

  /** Dial the call server and join. Resolves on socket-open (Join sent);
   *  rejects on dial failure — the caller keeps the honest avatar-presence
   *  face on rejection, never a fake connected state. */
  connect(
    url: string,
    callId: string,
    userId: string,
    displayName: string,
    livekit?: { url: string; token: string },
  ): Promise<void> {
    if (livekit !== undefined) {
      // The REAL plane, in parallel with the WS control dial. Failure is loud
      // and leaves lkLive=false — capture then refuses rather than silently
      // publishing nowhere ([[fallbacks-are-illegal-fail-loud]]).
      void this.connectLiveKit(livekit.url, livekit.token);
    }
    return this.connectWs(url, callId, userId, displayName);
  }

  private async connectLiveKit(url: string, token: string): Promise<void> {
    try {
      const room = new Room();
      // ACTIVE SPEAKERS — the audio-level signal LiveKit computes from real audio
      // tracks (what legacy's AudioStreamClient used). The token rail's `speaking`
      // flag lights a tile while a persona GENERATES TEXT, so every tile glowed at
      // once and nobody was audible (Joel, 2026-09-05). Tiles light on this only.
      room.on(RoomEvent.ActiveSpeakersChanged, (speakers) => {
        this.events.onActiveSpeakers?.(new Set(speakers.map((p) => p.identity)));
      });
      room.on(RoomEvent.TrackSubscribed, (track, _pub, participant) => {
        const kind = track.kind === Track.Kind.Video ? 'video' : 'audio';
        this.events.onTrack?.(participant.identity, kind, track);
      });
      room.on(RoomEvent.TrackUnsubscribed, (track, _pub, participant) => {
        const kind = track.kind === Track.Kind.Video ? 'video' : 'audio';
        this.events.onTrack?.(participant.identity, kind, undefined);
      });
      await room.connect(url, token);
      this.lkRoom = room;
      this.lkLive = true;
      // Tracks already live in the room replay through TrackSubscribed on
      // connect; nothing to enumerate by hand.
    } catch (err) {
      this.lkLive = false;
      console.error('LiveKit media plane connect failed — media stays on the WS tee this call:', err);
    }
  }

  private connectWs(url: string, callId: string, userId: string, displayName: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(url);
      ws.binaryType = 'arraybuffer';
      this.ws = ws;
      ws.onopen = () => {
        ws.send(
          JSON.stringify({
            type: 'Join',
            call_id: callId,
            user_id: userId,
            display_name: displayName,
            is_ai: false,
          }),
        );
        this.connected = true;
        this.events.onConnected?.();
        resolve();
      };
      ws.onerror = () => {
        if (!this.connected) reject(new Error(`call server unreachable at ${url}`));
      };
      ws.onclose = (e) => {
        this.connected = false;
        this.micLive = false;
        this.events.onClosed?.(this.closedByUs ? 'left' : `closed (${e.code})`);
      };
      ws.onmessage = (e) => {
        if (typeof e.data === 'string') this.handleText(e.data);
        else this.handleBinary(new Uint8Array(e.data as ArrayBuffer));
      };
    });
  }

  /** Start mic capture (AudioWorklet) and publish 16k PCM16 frames. Returns
   *  false with no side effects when the user denies the mic. */
  /** `mode` isolates the two capture paths for the node's self-exercise:
   *  'auto' (default) = LiveKit track when the media plane is live, else the
   *  worklet over the call socket; 'lk' / 'ws' force one path. */
  async startMic(mode: 'auto' | 'lk' | 'ws' = 'auto'): Promise<boolean> {
    // REAL plane first: the bridge's STT listener sits in the LiveKit room, so
    // a published mic track reaches citizens' ears with no WS PCM leg.
    console.warn(`[live-mic] start mode=${mode} lkRoom=${this.lkRoom !== undefined} lkLive=${this.lkLive} ws=${this.ws?.readyState}`);
    if (mode !== 'ws' && this.lkRoom !== undefined && this.lkLive) {
      try {
        this.lkMic = await createLocalAudioTrack();
        await this.lkRoom.localParticipant.publishTrack(this.lkMic);
        this.micLive = true;
        console.warn('[live-mic] livekit track published');
        return true;
      } catch (err) {
        console.warn('[live-mic] livekit publish failed:', err);
        return false;
      }
    }
    if (mode === 'lk') return false; // LiveKit-only was asked for and the plane is not live
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return false;
    try {
      this.micStream = await navigator.mediaDevices.getUserMedia({ audio: true });
    } catch {
      return false;
    }
    const ctx = await this.ensureAudio();
    const source = ctx.createMediaStreamSource(this.micStream);
    const capture = new AudioWorkletNode(ctx, 'pcm-capture');
    capture.port.onmessage = (e) => {
      const pcm = new Uint8Array(e.data as ArrayBuffer);
      const framed = new Uint8Array(1 + pcm.length);
      framed[0] = FRAME_AUDIO;
      framed.set(pcm, 1);
      if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(framed);
    };
    source.connect(capture);
    this.captureNode = capture;
    this.micLive = true;
    return true;
  }

  /** Camera capture state — true while frames are publishing. */
  camLive = false;
  private camStream?: MediaStream;
  private camTimer?: ReturnType<typeof setInterval>;
  private camSeq = 0;
  private camEpoch = Date.now();

  /** Start camera capture and publish RGBA frames on the SAME wire the Bevy
   *  avatar feed rides ([0x02][VideoFrameHeader 16B LE][rgba]) — humans are
   *  video participants inherently, no separate plane (Joel, 2026-08-31:
   *  "needs to be a positron thing inherent for people"). 320×180@12fps keeps
   *  a raw-RGBA upstream honest (~2.7MB/s, localhost-class); returns false
   *  with no side effects when the user denies the camera. */
  async startCamera(selfId?: string): Promise<boolean> {
    // REAL plane first: native WebRTC capture + hardware encode, published as
    // a UDP track. The canvas-RGBA-over-WS path below survives only as the
    // tee for cores without the media plane.
    if (this.lkRoom !== undefined && this.lkLive) {
      try {
        this.lkCam = await createLocalVideoTrack({
          resolution: { width: 640, height: 360, frameRate: 30 },
        });
        await this.lkRoom.localParticipant.publishTrack(this.lkCam);
        this.camLive = true;
        // Local echo: hand the local track up like any remote one.
        this.events.onTrack?.(selfId ?? 'local', 'video', this.lkCam as unknown as RemoteTrack);
        return true;
      } catch {
        return false; // permission denied or capture failure — honest no
      }
    }
    try {
      this.camStream = await navigator.mediaDevices.getUserMedia({
        video: { width: { ideal: 640 }, height: { ideal: 360 } },
      });
    } catch {
      return false;
    }
    const video = document.createElement('video');
    video.srcObject = this.camStream;
    video.muted = true;
    await video.play();
    const W = 320;
    const H = 180;
    const canvas = document.createElement('canvas');
    canvas.width = W;
    canvas.height = H;
    const ctx = canvas.getContext('2d', { willReadFrequently: true });
    if (!ctx) {
      this.stopCamera();
      return false;
    }
    this.camTimer = setInterval(() => {
      if (this.ws?.readyState !== WebSocket.OPEN) return;
      ctx.drawImage(video, 0, 0, W, H);
      const rgba = ctx.getImageData(0, 0, W, H).data;
      const frame = new Uint8Array(1 + 16 + rgba.length);
      const dv = new DataView(frame.buffer);
      frame[0] = FRAME_VIDEO;
      dv.setUint16(1, W, true);
      dv.setUint16(3, H, true);
      frame[5] = 0; // VideoPixelFormat::RGBA8
      frame[6] = 0; // flags reserved
      dv.setUint32(7, (Date.now() - this.camEpoch) >>> 0, true);
      dv.setUint32(11, this.camSeq, true);
      // bytes 15..17 reserved (already zero)
      frame.set(rgba, 17);
      this.ws.send(frame);
      // Local echo: the server broadcast is mix-minus (excludes self), so the
      // viewer's own tile animates from the same frame just published.
      if (selfId !== undefined) {
        this.events.onVideoFrame?.({
          senderId: selfId,
          width: W,
          height: H,
          pixelFormat: 0,
          sequence: this.camSeq,
          pixels: new Uint8Array(rgba.buffer.slice(0)),
        });
      }
      this.camSeq += 1;
    }, 1000 / 12);
    this.camLive = true;
    return true;
  }

  stopCamera(): void {
    if (this.lkCam !== undefined) {
      void this.lkRoom?.localParticipant.unpublishTrack(this.lkCam);
      this.lkCam.stop();
      this.lkCam = undefined;
    }
    if (this.camTimer !== undefined) clearInterval(this.camTimer);
    this.camTimer = undefined;
    this.camStream?.getTracks().forEach((t) => t.stop());
    this.camStream = undefined;
    this.camLive = false;
  }

  stopMic(): void {
    if (this.lkMic !== undefined) {
      void this.lkRoom?.localParticipant.unpublishTrack(this.lkMic);
      this.lkMic.stop();
      this.lkMic = undefined;
    }
    this.captureNode?.disconnect();
    this.captureNode = undefined;
    this.micStream?.getTracks().forEach((t) => t.stop());
    this.micStream = undefined;
    this.micLive = false;
  }

  leave(): void {
    this.closedByUs = true;
    this.stopMic();
    this.stopCamera();
    void this.lkRoom?.disconnect();
    this.lkRoom = undefined;
    this.lkLive = false;
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: 'Leave' }));
    }
    this.ws?.close();
    void this.ctx?.close();
    this.ctx = undefined;
  }

  private async ensureAudio(): Promise<AudioContext> {
    if (this.ctx) return this.ctx;
    const ctx = new AudioContext();
    const blob = new Blob([WORKLET_SOURCE], { type: 'application/javascript' });
    const url = URL.createObjectURL(blob);
    try {
      await ctx.audioWorklet.addModule(url);
    } finally {
      URL.revokeObjectURL(url);
    }
    const player = new AudioWorkletNode(ctx, 'pcm-player');
    player.connect(ctx.destination);
    this.player = player;
    this.ctx = ctx;
    return ctx;
  }

  private handleText(raw: string): void {
    let msg: Record<string, unknown>;
    try {
      msg = JSON.parse(raw) as Record<string, unknown>;
    } catch {
      return;
    }
    // serde(tag = "type") with default casing — variant names verbatim.
    const t = (msg.type ?? '') as string;
    if (t === 'ParticipantJoined') {
      this.events.onParticipantJoined?.(String(msg.user_id ?? ''), String(msg.display_name ?? ''));
    } else if (t === 'ParticipantLeft') {
      this.events.onParticipantLeft?.(String(msg.user_id ?? ''));
    } else if (t === 'AvatarUpdate') {
      this.events.onAvatar?.({
        personaId: String(msg.persona_id ?? ''),
        speaking: Boolean(msg.speaking),
        listening: Boolean(msg.listening),
        emotion: String(msg.emotion ?? ''),
        viseme: Number(msg.viseme ?? 0),
        visemeWeight: Number(msg.viseme_weight ?? 0),
      });
    } else if (t === 'Transcription') {
      const userId = String(msg.user_id ?? '');
      const text = String(msg.text ?? '');
      this.events.onTranscription?.(userId, text);
      this.events.onDelta?.({
        roomId: '',
        senderId: userId,
        streamId: `stt-${userId}`,
        seq: 0,
        token: text.endsWith(' ') ? text : text + ' ',
        done: false,
      });
    }
  }

  private handleBinary(data: Uint8Array): void {
    if (data.length < 2) return;
    const kind = data[0];
    const idLen = data[1] ?? 0;
    if (data.length < 2 + idLen) return;
    const senderId = new TextDecoder().decode(data.subarray(2, 2 + idLen));
    const payload = data.subarray(2 + idLen);
    if (kind === FRAME_AUDIO) {
      // Per-sender PCM16 → hand to the player worklet (browser-side mix is
      // additive; the server already excludes our own audio — mix-minus).
      void this.ensureAudio().then(() => {
        const copy = payload.slice();
        this.player?.port.postMessage(copy.buffer, [copy.buffer]);
      });
    } else if (kind === FRAME_VIDEO && payload.length >= 16) {
      const view = new DataView(payload.buffer, payload.byteOffset, payload.byteLength);
      this.events.onVideoFrame?.({
        senderId,
        width: view.getUint16(0, true),
        height: view.getUint16(2, true),
        pixelFormat: payload[4] ?? 0,
        sequence: view.getUint32(10, true),
        pixels: payload.slice(16),
      });
    }
  }
}
