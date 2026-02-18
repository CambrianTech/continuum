/**
 * Academy Types - Shared types for the Academy Dojo dual-sentinel architecture
 *
 * The Academy is a self-sustaining learning system where:
 * - A Teacher Sentinel synthesizes training data and examinations using LLM
 * - A Student Sentinel trains on that data and proves mastery through exams
 * - Inter-sentinel communication flows through emit/watch events
 *
 * Like Plato's Academy: the teacher adapts curriculum based on examination results,
 * generating more data where the student is weak.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

// ============================================================================
// Event Taxonomy — All events scoped by session ID
// ============================================================================

/**
 * Generate a scoped Academy event name.
 *
 * All Academy events follow the pattern: `academy:{sessionId}:{action}`
 * This enables multiple concurrent Academy sessions without event collision.
 */
export function academyEvent(sessionId: string, action: AcademyEventAction): string {
  return `academy:${sessionId}:${action}`;
}

/**
 * All possible Academy event actions
 */
export type AcademyEventAction =
  | 'curriculum:ready'
  | 'dataset:ready'
  | 'training:started'
  | 'training:progress'
  | 'training:complete'
  | 'exam:ready'
  | 'exam:responses'
  | 'exam:graded'
  | 'topic:passed'
  | 'topic:remediate'
  | 'inference:demo'
  | 'quality:gate:failed'
  | 'session:complete'
  | 'session:failed';

// ============================================================================
// Academy Session Config
// ============================================================================

/**
 * Configuration for an Academy training session
 */
export interface AcademyConfig {
  /** Maximum attempts per topic before failure (default: 3) */
  maxTopicAttempts: number;

  /** Score required to pass an exam, 0-100 (default: 70) */
  passingScore: number;

  /** Training epochs per round (default: 3) */
  epochs: number;

  /** LoRA rank for training (default: 32) */
  rank: number;

  /** Learning rate (default: 0.0001) */
  learningRate: number;

  /** Training batch size (default: 4) */
  batchSize: number;

  /** Number of training examples to synthesize per topic (default: 10) */
  examplesPerTopic: number;

  /** Number of exam questions per topic (default: 10) */
  questionsPerExam: number;

  /** LLM model for teacher (curriculum design, data synthesis, grading) */
  teacherModel?: string;

  /** LLM provider for teacher */
  teacherProvider?: string;
}

/**
 * Default Academy configuration
 */
export const DEFAULT_ACADEMY_CONFIG: AcademyConfig = {
  maxTopicAttempts: 3,
  passingScore: 70,
  epochs: 3,
  rank: 32,
  learningRate: 0.0001,
  batchSize: 4,
  examplesPerTopic: 10,
  questionsPerExam: 10,
};

// ============================================================================
// Academy Session Status
// ============================================================================

export type AcademySessionStatus =
  | 'pending'       // Created, not yet started
  | 'curriculum'    // Teacher designing curriculum
  | 'training'      // Student training on current topic
  | 'examining'     // Student taking exam for current topic
  | 'complete'      // All topics passed
  | 'failed';       // Max attempts exceeded

export const VALID_SESSION_STATUSES: AcademySessionStatus[] = [
  'pending', 'curriculum', 'training', 'examining', 'complete', 'failed',
];

// ============================================================================
// Curriculum Types
// ============================================================================

export type CurriculumTopicStatus =
  | 'pending'       // Not yet attempted
  | 'training'      // Currently training
  | 'examining'     // Currently examining
  | 'passed'        // Student passed
  | 'failed';       // Student failed all attempts

/**
 * A single topic in the curriculum
 */
export interface CurriculumTopic {
  /** Topic name (e.g., "Generic type constraints") */
  name: string;

  /** Description of what this topic covers */
  description: string;

  /** Difficulty level */
  difficulty: 'beginner' | 'intermediate' | 'advanced';

  /** Path to synthesized JSONL dataset (populated after synthesis) */
  datasetPath?: string;

  /** Current status of this topic */
  status: CurriculumTopicStatus;

  /** Number of exam attempts for this topic */
  attempts: number;

  /** Best exam score for this topic (0-100) */
  bestScore: number;
}

// ============================================================================
// Examination Types
// ============================================================================

/**
 * A single exam question
 */
export interface ExamQuestion {
  /** The question text */
  question: string;

  /** Expected answer (for grading reference) */
  expectedAnswer: string;

  /** Category within the topic */
  category: string;
}

/**
 * A student's response to an exam question
 */
export interface ExamResponse {
  /** Index into the questions array */
  questionIndex: number;

  /** The student's answer */
  studentAnswer: string;

  /** Score for this answer (0-100) */
  score: number;

  /** Grading feedback */
  feedback: string;
}

// ============================================================================
// Pipeline Config Types
// ============================================================================

/**
 * Configuration for building the teacher sentinel pipeline
 */
export interface TeacherPipelineConfig {
  sessionId: UUID;
  skill: string;
  personaName: string;
  baseModel: string;
  config: AcademyConfig;
  /**
   * Data sources for knowledge synthesis (optional).
   * When provided, the teacher explores these sources first and grounds
   * all training data in extracted facts. When absent, uses pure LLM generation.
   */
  dataSources?: import('./KnowledgeTypes').DataSourceConfig[];
  /**
   * Whether to auto-generate a persistent benchmark from extracted knowledge.
   * Only relevant when dataSources are provided.
   */
  generateBenchmark?: boolean;
}

/**
 * Configuration for building the student sentinel pipeline
 */
export interface StudentPipelineConfig {
  sessionId: UUID;
  personaId: UUID;
  personaName: string;
  baseModel: string;
  config: AcademyConfig;
}

// ============================================================================
// Event Payloads
// ============================================================================

export interface CurriculumReadyPayload {
  curriculumId: UUID;
  topics: CurriculumTopic[];
  totalTopics: number;
}

export interface DatasetReadyPayload {
  datasetPath: string;
  topicIndex: number;
  topicName: string;
  exampleCount: number;
}

export interface TrainingCompletePayload {
  layerId: UUID;
  topicIndex: number;
  metrics: {
    finalLoss: number;
    trainingTime: number;
    examplesProcessed: number;
    epochs: number;
  };
}

export interface ExamReadyPayload {
  examId: UUID;
  topicIndex: number;
  questions: ExamQuestion[];
}

export interface ExamResponsesPayload {
  examId: UUID;
  topicIndex: number;
  responses: ExamResponse[];
}

export interface ExamGradedPayload {
  examId: UUID;
  topicIndex: number;
  overallScore: number;
  passed: boolean;
  round: number;
  feedback: string;
}

export interface InferenceDemoPayload {
  sessionId: UUID;
  personaId: UUID;
  topicIndex: number;
  topicName: string;
  baselineScore: number;
  adaptedScore: number;
  improvement: number;
  summary: string;
  sampleQuestion: string;
  sampleBaselineAnswer: string;
  sampleAdaptedAnswer: string;
}

export interface QualityGateFailedPayload {
  sessionId: UUID;
  personaId: UUID;
  topicIndex: number;
  topicName: string;
  baselineScore: number;
  adaptedScore: number;
  improvement: number;
  summary: string;
}

export interface TopicRemediatePayload {
  sessionId: UUID;
  topicIndex: number;
  round: number;
  feedback: string;
  weakAreas: string[];
}

export interface RemediationDatasetReadyPayload extends DatasetReadyPayload {
  isRemediation: true;
  round: number;
}
