/**
 * LiveKit Participant Types — shared between browser and server.
 *
 * Participant classification uses LiveKit's metadata field (JSON) instead of
 * identity string prefixes. This eliminates fragile string parsing and gives
 * each participant a proper typed role.
 *
 * Metadata is set in the JWT token at connection time and readable via
 * `participant.metadata` on both Rust and JS SDKs.
 */

/** LiveKit participant role — determines UI visibility and audio routing. */
export enum ParticipantRole {
  /** Human user with microphone/camera */
  Human = 'human',
  /** AI persona agent — publishes TTS audio, visible in participant grid */
  AIPersona = 'ai_persona',
  /** STT listener — subscribe-only agent for VAD/STT, invisible in UI */
  STTListener = 'stt_listener',
  /** Ambient audio source (rain, hold music) — invisible in UI */
  AmbientAudio = 'ambient_audio',
}

/** Metadata attached to every LiveKit participant via JWT token. */
export interface ParticipantMetadata {
  role: ParticipantRole;
}

/** Parse participant metadata from LiveKit's metadata JSON string. */
export function parseParticipantMetadata(metadata: string | undefined): ParticipantMetadata | null {
  if (!metadata) return null;
  try {
    const parsed = JSON.parse(metadata);
    if (parsed.role && Object.values(ParticipantRole).includes(parsed.role)) {
      return parsed as ParticipantMetadata;
    }
    return null;
  } catch {
    return null;
  }
}

/** Check if a participant should appear in the UI participant grid. */
export function isVisibleParticipant(metadata: ParticipantMetadata | null): boolean {
  if (!metadata) return true; // Unknown = visible (backwards compat)
  return metadata.role === ParticipantRole.Human || metadata.role === ParticipantRole.AIPersona;
}

/** Check if a participant's audio should be transcribed by STT. */
export function shouldTranscribeAudio(metadata: ParticipantMetadata | null): boolean {
  if (!metadata) return true; // Unknown = probably human
  return metadata.role === ParticipantRole.Human;
}
