/**
 * Competition Types — Multi-persona competition and evolution tournament types
 *
 * Competitions pit N personas against the same curriculum from a single teacher.
 * Each persona gets their own student sentinel, all sharing the same teacher's
 * exam questions. Rankings are computed from exam scores, training metrics,
 * and inference quality.
 *
 * Evolution tournaments run multiple rounds of competition, with the weakest
 * performers receiving targeted remediation between rounds.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { AcademyConfig } from './AcademyTypes';

// ============================================================================
// Competition Status
// ============================================================================

export type CompetitionStatus =
  | 'pending'         // Created, sentinels not yet spawned
  | 'curriculum'      // Teacher designing shared curriculum
  | 'training'        // Students training on current topic
  | 'examining'       // Students taking exams
  | 'ranking'         // All topics done, computing final rankings
  | 'complete'        // Rankings finalized
  | 'failed';         // Unrecoverable error

export const VALID_COMPETITION_STATUSES: CompetitionStatus[] = [
  'pending', 'curriculum', 'training', 'examining', 'ranking', 'complete', 'failed',
];

// ============================================================================
// Competitor Entry
// ============================================================================

/**
 * A single competitor in the competition.
 * Tracks per-persona sentinel handles and cumulative scores.
 */
export interface CompetitorEntry {
  /** Persona ID (the student) */
  personaId: UUID;

  /** Persona display name */
  personaName: string;

  /** Sentinel handle for this persona's student pipeline */
  studentHandle: string;

  /** Academy session ID for this competitor */
  sessionId: UUID;

  /** Per-topic exam scores (index = topic index, value = best score 0-100) */
  topicScores: number[];

  /** Number of topics passed */
  topicsPassed: number;

  /** Total exam attempts across all topics */
  totalAttempts: number;

  /** Average exam score across all graded topics */
  averageScore: number;

  /** Final rank (1 = best, assigned during ranking phase) */
  rank: number;

  /** Total training time in ms */
  totalTrainingTimeMs: number;

  /** Layer IDs produced by training */
  layerIds: UUID[];
}

// ============================================================================
// Gap Analysis
// ============================================================================

/**
 * Performance gap for a single topic, comparing one persona against the field.
 */
export interface TopicGap {
  /** Topic index in curriculum */
  topicIndex: number;

  /** Topic name */
  topicName: string;

  /** This persona's best score */
  personaScore: number;

  /** Best score across all competitors for this topic */
  fieldBest: number;

  /** Average score across all competitors for this topic */
  fieldAverage: number;

  /** Gap from field best (negative = behind) */
  gapFromBest: number;

  /** Gap from field average */
  gapFromAverage: number;

  /** Weak areas identified by grading (from exam feedback) */
  weakAreas: string[];
}

/**
 * Full gap analysis for a single persona across all topics.
 */
export interface GapAnalysis {
  /** Persona being analyzed */
  personaId: UUID;
  personaName: string;

  /** Competition this analysis belongs to */
  competitionId: UUID;

  /** Per-topic gap breakdown */
  topicGaps: TopicGap[];

  /** Overall rank in competition */
  overallRank: number;

  /** Overall average score */
  overallAverage: number;

  /** Topics where persona is weakest (sorted by gap) */
  weakestTopics: string[];

  /** Topics where persona is strongest */
  strongestTopics: string[];

  /** Recommended remediation focus areas */
  remediationPriorities: string[];
}

// ============================================================================
// Evolution Tournament
// ============================================================================

/**
 * A single round in an evolution tournament.
 * Each round is a full competition with rankings.
 */
export interface TournamentRound {
  /** Round number (1-based) */
  round: number;

  /** Competition ID for this round */
  competitionId: UUID;

  /** Rankings snapshot at end of round */
  rankings: TournamentRanking[];

  /** Whether remediation was applied after this round */
  remediationApplied: boolean;

  /** Round start timestamp */
  startedAt: string;

  /** Round end timestamp */
  completedAt?: string;
}

/**
 * A persona's ranking within a tournament round.
 */
export interface TournamentRanking {
  /** Persona ID */
  personaId: UUID;

  /** Persona display name */
  personaName: string;

  /** Rank this round (1 = best) */
  rank: number;

  /** Score this round */
  score: number;

  /** Score change from previous round (null for round 1) */
  scoreDelta: number | null;

  /** Rank change from previous round (positive = improved) */
  rankDelta: number | null;
}

export type TournamentStatus =
  | 'pending'
  | 'running'
  | 'complete'
  | 'failed';

export const VALID_TOURNAMENT_STATUSES: TournamentStatus[] = [
  'pending', 'running', 'complete', 'failed',
];

// ============================================================================
// Competition Event Actions (extend AcademyEventAction taxonomy)
// ============================================================================

/**
 * Competition-scoped event actions.
 * Events follow: `competition:{competitionId}:{action}`
 */
export type CompetitionEventAction =
  | 'started'               // Competition spawned all sentinels
  | 'student:joined'        // A student sentinel started
  | 'student:complete'      // A student finished all topics
  | 'ranking:computed'      // Rankings calculated
  | 'complete'              // Competition finished
  | 'failed';               // Competition failed

/**
 * Generate a scoped competition event name.
 */
export function competitionEvent(competitionId: string, action: CompetitionEventAction): string {
  return `competition:${competitionId}:${action}`;
}

// ============================================================================
// Competition Config
// ============================================================================

/**
 * Configuration for a competition, extending AcademyConfig with
 * competition-specific parameters.
 */
export interface CompetitionConfig extends AcademyConfig {
  /** Number of tournament rounds (default: 1 = single competition) */
  tournamentRounds: number;

  /** Apply remediation between tournament rounds (default: true) */
  remediateBetweenRounds: boolean;
}

export const DEFAULT_COMPETITION_CONFIG: CompetitionConfig = {
  maxTopicAttempts: 3,
  passingScore: 70,
  epochs: 3,
  rank: 32,
  learningRate: 0.0001,
  batchSize: 4,
  examplesPerTopic: 10,
  questionsPerExam: 10,
  tournamentRounds: 1,
  remediateBetweenRounds: true,
};

// ============================================================================
// Event Payloads
// ============================================================================

export interface CompetitionStartedPayload {
  competitionId: UUID;
  skill: string;
  competitorCount: number;
  competitors: Array<{ personaId: UUID; personaName: string }>;
}

export interface CompetitionRankingPayload {
  competitionId: UUID;
  rankings: TournamentRanking[];
  round: number;
}

export interface CompetitionCompletePayload {
  competitionId: UUID;
  skill: string;
  finalRankings: TournamentRanking[];
  totalRounds: number;
}
