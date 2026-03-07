/**
 * VoiceSessionTimeline - Sequenced, cursor-tracked voice conversation timeline
 *
 * Replaces the naive recentUtterances[] array in VoiceOrchestrator's SessionContext.
 *
 * Problems solved:
 * 1. Temporal tracking — each utterance gets a monotonic sequence number
 * 2. Per-persona cursors — each persona knows WHERE in the timeline they are
 * 3. Fragment consolidation at ingestion — speech fragments merged into turns on write, not read
 * 4. Bounded growth — sliding window with configurable max, old turns drop off
 * 5. Single source of truth — no more 3x duplicate push+shift blocks
 *
 * The persona sees: "here's what happened since you last looked" + backfill context.
 * Not "here's the last 20 utterances regardless of what you've already processed."
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('VoiceSessionTimeline', 'voice');

/**
 * A sequenced utterance in the timeline.
 * Sequence numbers are monotonic per-session — they never reset or reuse.
 */
export interface TimelineEntry {
  /** Monotonic sequence number within this session */
  seq: number;
  /** Voice session ID */
  sessionId: UUID;
  /** Who spoke */
  speakerId: UUID;
  speakerName: string;
  speakerType: 'human' | 'persona' | 'agent';
  /** Consolidated transcript (fragments already merged with "..." markers) */
  transcript: string;
  /** Transcription confidence (0-1) */
  confidence: number;
  /** Wall clock timestamp of the turn start (ms since epoch) */
  timestamp: number;
  /** Number of raw fragments that were merged into this entry */
  fragmentCount: number;
}

/**
 * Raw utterance event (input from transcription/AI speech).
 * Same shape as VoiceOrchestrator's UtteranceEvent.
 */
export interface RawUtterance {
  sessionId: string;
  speakerId: string;
  speakerName: string;
  speakerType: 'human' | 'persona' | 'agent';
  transcript: string;
  confidence: number;
  timestamp: number;
}

/**
 * Cursor-relative query result.
 * Contains both new utterances since cursor AND optional backfill for context.
 */
export interface TimelineSlice {
  /** New entries since the persona's cursor */
  newEntries: TimelineEntry[];
  /** Backfill entries for conversational context (before cursor, most recent first) */
  backfill: TimelineEntry[];
  /** The sequence number to advance the cursor to after processing */
  headSeq: number;
  /** Total turns in the session (for metadata) */
  totalTurns: number;
}

/** Configuration for timeline behavior */
interface TimelineConfig {
  /** Max consolidated turns to retain (oldest drop off). Default: 100 */
  maxTurns: number;
  /** Max gap (ms) between same-speaker fragments to merge. Default: 2000 */
  mergeGapMs: number;
  /** Default backfill count when a persona has no cursor yet. Default: 15 */
  defaultBackfill: number;
}

const DEFAULT_CONFIG: TimelineConfig = {
  maxTurns: 100,
  mergeGapMs: 2000,
  defaultBackfill: 15,
};

export class VoiceSessionTimeline {
  private readonly sessionId: UUID;
  private readonly config: TimelineConfig;

  /** Consolidated turns, ordered by seq. Bounded by maxTurns. */
  private entries: TimelineEntry[] = [];

  /** Next sequence number to assign */
  private _nextSeq = 1;

  /** Per-persona cursor: personaId → last processed seq number */
  private cursors: Map<UUID, number> = new Map();

  /** Total raw fragments ingested (for diagnostics) */
  private _totalFragments = 0;

  constructor(sessionId: UUID, config?: Partial<TimelineConfig>) {
    this.sessionId = sessionId;
    this.config = { ...DEFAULT_CONFIG, ...config };
  }

  /**
   * Ingest a raw utterance into the timeline.
   *
   * If the previous entry is from the same speaker within mergeGapMs,
   * the fragment is consolidated into that entry with a "..." continuity marker.
   * Otherwise, a new turn is created.
   *
   * Returns the entry that was created or extended.
   */
  append(utterance: RawUtterance): TimelineEntry {
    this._totalFragments++;
    const last = this.entries.length > 0 ? this.entries[this.entries.length - 1] : null;

    const sameSpeaker = last && last.speakerId === utterance.speakerId;
    const gap = last ? utterance.timestamp - last.timestamp : Infinity;
    const withinMergeWindow = gap < this.config.mergeGapMs;

    if (sameSpeaker && withinMergeWindow) {
      // Merge into existing turn.
      // Short gaps (<500ms) = continuous speech captured in chunks → simple space join.
      // Longer gaps (500ms-2s) = speaker paused briefly but is still going → "..." marker.
      // The "..." tells the persona "this is a brief pause, not a new thought."
      const PAUSE_THRESHOLD_MS = 500;
      const joiner = gap > PAUSE_THRESHOLD_MS ? '... ' : ' ';
      last.transcript = last.transcript.trimEnd() + joiner + utterance.transcript.trimStart();
      last.fragmentCount++;
      // Confidence: keep highest across fragments
      last.confidence = Math.max(last.confidence, utterance.confidence);
      return last;
    }

    // Previous turn is now finalized — clean up any "..." markers.
    // The "..." was useful while the turn was still building (signals "more coming"),
    // but once complete the transcript should read naturally as connected text.
    if (last && last.fragmentCount > 1) {
      last.transcript = last.transcript.replace(/\.\.\.\s*/g, ' ').replace(/\s{2,}/g, ' ').trim();
    }

    // New turn
    const entry: TimelineEntry = {
      seq: this._nextSeq++,
      sessionId: this.sessionId,
      speakerId: utterance.speakerId as UUID,
      speakerName: utterance.speakerName,
      speakerType: utterance.speakerType,
      transcript: utterance.transcript,
      confidence: utterance.confidence,
      timestamp: utterance.timestamp,
      fragmentCount: 1,
    };

    this.entries.push(entry);

    // Enforce sliding window
    if (this.entries.length > this.config.maxTurns) {
      const dropped = this.entries.shift()!;
      // Advance any cursors that pointed at the dropped entry
      for (const [personaId, cursor] of this.cursors) {
        if (cursor <= dropped.seq) {
          this.cursors.set(personaId, dropped.seq);
        }
      }
    }

    return entry;
  }

  /**
   * Get a timeline slice for a specific persona.
   *
   * Returns entries SINCE the persona's cursor (new stuff) plus backfill
   * context (older entries the persona has already seen, for conversational continuity).
   *
   * @param personaId - The persona requesting context
   * @param backfillCount - How many entries before cursor to include (default: config.defaultBackfill)
   * @param maxNew - Max new entries to return (prevents explosion after long absence)
   */
  sliceFor(personaId: UUID, backfillCount?: number, maxNew?: number): TimelineSlice {
    const cursor = this.cursors.get(personaId) ?? 0;
    const backfill = backfillCount ?? this.config.defaultBackfill;

    // Split entries into before-cursor and after-cursor
    const newEntries: TimelineEntry[] = [];
    const beforeCursor: TimelineEntry[] = [];

    for (const entry of this.entries) {
      if (entry.seq > cursor) {
        newEntries.push(entry);
      } else {
        beforeCursor.push(entry);
      }
    }

    // Cap new entries if persona was away too long
    const cappedNew = maxNew ? newEntries.slice(-maxNew) : newEntries;

    // Backfill: most recent N entries from before cursor
    const backfillEntries = beforeCursor.slice(-backfill);

    // Head seq = highest seq in the timeline (what cursor should advance to)
    const headSeq = this.entries.length > 0
      ? this.entries[this.entries.length - 1].seq
      : 0;

    return {
      newEntries: cappedNew,
      backfill: backfillEntries,
      headSeq,
      totalTurns: this.entries.length,
    };
  }

  /**
   * Advance a persona's cursor after processing.
   * Call this AFTER the persona has generated a response using the slice.
   */
  advanceCursor(personaId: UUID, seq: number): void {
    const current = this.cursors.get(personaId) ?? 0;
    if (seq > current) {
      this.cursors.set(personaId, seq);
    }
  }

  /**
   * Get most recent N entries (no cursor awareness — for export, debugging).
   */
  recent(limit: number): TimelineEntry[] {
    return this.entries.slice(-limit);
  }

  /** Current head sequence number */
  get headSeq(): number {
    return this._nextSeq - 1;
  }

  /** Number of consolidated turns currently in the timeline */
  get length(): number {
    return this.entries.length;
  }

  /** Total raw fragments ingested since session start */
  get totalFragments(): number {
    return this._totalFragments;
  }

  /** Total consolidated turns created (including dropped ones) */
  get totalTurns(): number {
    return this._nextSeq - 1;
  }

  /** Remove a persona's cursor (when they leave the call) */
  removeCursor(personaId: UUID): void {
    this.cursors.delete(personaId);
  }

  /** Clear all state (session end) */
  clear(): void {
    this.entries = [];
    this.cursors.clear();
    this._nextSeq = 1;
    this._totalFragments = 0;
  }

  /**
   * Backwards-compatible: return recent utterances as the old UtteranceEvent[] shape.
   * Used by VoiceConversationSource until it's migrated to use sliceFor().
   */
  getRecentUtterances(limit: number): RawUtterance[] {
    return this.recent(limit).map(entry => ({
      sessionId: entry.sessionId,
      speakerId: entry.speakerId,
      speakerName: entry.speakerName,
      speakerType: entry.speakerType,
      transcript: entry.transcript,
      confidence: entry.confidence,
      timestamp: entry.timestamp,
    }));
  }
}
