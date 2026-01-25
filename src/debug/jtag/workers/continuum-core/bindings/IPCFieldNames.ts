/**
 * IPC Field Name Constants
 *
 * MUST MATCH EXACTLY: workers/continuum-core/src/ipc/mod.rs constants
 * Source of truth: Rust (VOICE_RESPONSE_FIELD_RESPONDER_IDS)
 *
 * DO NOT use magic strings - import from here.
 * DO NOT modify without updating Rust constant first.
 */

/**
 * Voice IPC Response Fields
 * These values MUST match the constants defined in continuum-core/src/ipc/mod.rs
 */
export const VOICE_RESPONSE_FIELDS = {
  /**
   * Array of AI participant UUIDs (broadcast model)
   * Rust constant: VOICE_RESPONSE_FIELD_RESPONDER_IDS
   */
  RESPONDER_IDS: 'responder_ids',
} as const;

export type VoiceResponseField = typeof VOICE_RESPONSE_FIELDS[keyof typeof VOICE_RESPONSE_FIELDS];
