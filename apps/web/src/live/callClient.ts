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
  /** True while the mic worklet is publishing. */
  micLive = false;

  constructor(events: CallClientEvents) {
    this.events = events;
  }

  /** Dial the call server and join. Resolves on socket-open (Join sent);
   *  rejects on dial failure — the caller keeps the honest avatar-presence
   *  face on rejection, never a fake connected state. */
  connect(url: string, callId: string, userId: string, displayName: string): Promise<void> {
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
  async startMic(): Promise<boolean> {
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

  stopMic(): void {
    this.captureNode?.disconnect();
    this.captureNode = undefined;
    this.micStream?.getTracks().forEach((t) => t.stop());
    this.micStream = undefined;
    this.micLive = false;
  }

  leave(): void {
    this.closedByUs = true;
    this.stopMic();
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
