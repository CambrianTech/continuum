/**
 * Knowledge Types — Intermediate representation between source exploration and synthesis
 *
 * These types define the data flow for knowledge synthesis:
 * 1. DataSourceConfig describes WHAT to explore (repo, web, docs, conversations)
 * 2. ExtractedFact is a single verified fact extracted from a source
 * 3. SourceKnowledge aggregates extracted facts with metadata
 * 4. BenchmarkDefinition persists auto-generated test suites
 * 5. BenchmarkResult records per-persona performance against benchmarks
 *
 * SourceKnowledge is EPHEMERAL — exists only within pipeline execution.
 * BenchmarkDefinition is PERSISTENT — stored in data layer, reusable across sessions.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

// ============================================================================
// Data Source Configuration — What to explore
// ============================================================================

/**
 * Git repository source configuration
 */
export interface GitRepoSourceConfig {
  type: 'git-repo';
  /** Path to the git repository */
  repoPath: string;
  /** Glob patterns for files to read (e.g., ["*.ts", "*.md"]) */
  fileGlobs?: string[];
  /** How many git log entries to read (default: 30) */
  gitLogDepth?: number;
  /** Max files to read (default: 15) */
  maxFiles?: number;
}

/**
 * Web research source configuration
 */
export interface WebResearchSourceConfig {
  type: 'web-research';
  /** Search queries to execute */
  searchQueries: string[];
  /** Limit results to these domains (optional) */
  domains?: string[];
  /** Max pages to fetch per query (default: 3) */
  maxPagesPerQuery?: number;
}

/**
 * Conversation log source configuration
 */
export interface ConversationLogSourceConfig {
  type: 'conversation-log';
  /** Paths to conversation log files (markdown, JSONL, etc.) */
  paths: string[];
}

/**
 * Document set source configuration
 */
export interface DocumentSetSourceConfig {
  type: 'document-set';
  /** Paths to document files or directories */
  paths: string[];
}

/**
 * Pure LLM generation — no source exploration, teacher invents from training data
 */
export interface PureGenerationSourceConfig {
  type: 'pure-generation';
}

/**
 * Union of all data source configurations.
 * The teacher sentinel uses these to determine what exploration steps to run.
 */
export type DataSourceConfig =
  | GitRepoSourceConfig
  | WebResearchSourceConfig
  | ConversationLogSourceConfig
  | DocumentSetSourceConfig
  | PureGenerationSourceConfig;

// ============================================================================
// Extracted Knowledge — What was found
// ============================================================================

/**
 * A single verified fact extracted from a data source.
 * These are the atomic units of knowledge that ground training data synthesis.
 */
export interface ExtractedFact {
  /** The fact statement (e.g., "The CEO of Nexaflux is Dr. Elena Vasquez") */
  statement: string;

  /** Confidence level from extraction (0-1) */
  confidence: number;

  /** Where this fact came from */
  source: FactSource;

  /** Category for curriculum design (e.g., "architecture", "api", "history") */
  category: string;
}

/**
 * Source attribution for an extracted fact
 */
export interface FactSource {
  /** Source type that produced this fact */
  sourceType: DataSourceConfig['type'];

  /** File path, URL, or identifier */
  location: string;

  /** Raw excerpt from source that contains/supports the fact */
  excerpt?: string;
}

/**
 * Aggregated knowledge from exploring one or more data sources.
 * EPHEMERAL — exists only during pipeline execution, not persisted.
 */
export interface SourceKnowledge {
  /** Human-readable summary of what was learned */
  summary: string;

  /** Extracted facts from all sources */
  facts: ExtractedFact[];

  /** What sources were explored */
  sourcesExplored: SourceExplorationRecord[];

  /** Total raw content size processed (bytes) */
  totalContentSize: number;

  /** When this knowledge was extracted */
  extractedAt: string;
}

/**
 * Record of a single source exploration (for provenance tracking)
 */
export interface SourceExplorationRecord {
  /** Source config that was explored */
  config: DataSourceConfig;

  /** How many facts were extracted from this source */
  factsExtracted: number;

  /** How many raw items were processed (files, pages, messages) */
  itemsProcessed: number;

  /** Duration of exploration in milliseconds */
  durationMs: number;
}

// ============================================================================
// Benchmark Types — Persistent, reusable test suites
// ============================================================================

/**
 * A single benchmark question with expected answer and rubric
 */
export interface BenchmarkQuestion {
  /** The question to ask */
  question: string;

  /** The expected/ideal answer */
  expectedAnswer: string;

  /** Grading rubric — what to look for in the answer */
  rubric: string;

  /** Category within the domain (for gap analysis) */
  category: string;

  /** Difficulty level */
  difficulty: 'easy' | 'medium' | 'hard';

  /** Which fact(s) this question tests (indices into SourceKnowledge.facts) */
  factIndices?: number[];
}

/**
 * A persistent benchmark definition — auto-generated test suite for a knowledge domain.
 * Stored in the data layer, reusable across sessions and personas.
 */
export interface BenchmarkDefinition {
  /** Unique identifier */
  id?: UUID;

  /** Human-readable name (e.g., "Nexaflux Corporation Knowledge") */
  name: string;

  /** Domain this benchmark tests */
  domain: string;

  /** The questions */
  questions: BenchmarkQuestion[];

  /** Summary of the source knowledge this was generated from */
  knowledgeSummary: string;

  /** Number of facts the benchmark covers */
  factCount: number;

  /** When this benchmark was created */
  createdAt: string;

  /** Who/what created this benchmark */
  createdBy: string;

  /** Version for iterative improvement */
  version: number;
}

/**
 * Result of running a persona against a benchmark
 */
export interface BenchmarkResult {
  /** Benchmark this result is for */
  benchmarkId: UUID;

  /** Benchmark name (denormalized for convenience) */
  benchmarkName: string;

  /** Persona that was tested */
  personaId: UUID;

  /** Persona name (denormalized) */
  personaName: string;

  /** Overall score (0-100) */
  overallScore: number;

  /** Per-question scores */
  questionScores: QuestionScore[];

  /** Per-category average scores */
  categoryScores: Record<string, number>;

  /** Which adapter (if any) was active during the test */
  adapterId?: UUID;

  /** When this benchmark was run */
  runAt: string;

  /** Duration of the benchmark run in milliseconds */
  durationMs: number;
}

/**
 * Score for a single benchmark question
 */
export interface QuestionScore {
  /** Index into BenchmarkDefinition.questions */
  questionIndex: number;

  /** Score for this question (0-100) */
  score: number;

  /** The persona's actual answer */
  answer: string;

  /** Grading feedback */
  feedback: string;
}

// ============================================================================
// Pipeline Integration Types
// ============================================================================

/**
 * Extended TeacherPipelineConfig with knowledge synthesis support.
 * When dataSources are provided, the teacher explores them before synthesis.
 */
export interface KnowledgeSynthesisConfig {
  /** Data sources to explore (optional — absent = pure generation) */
  dataSources?: DataSourceConfig[];

  /** Maximum total facts to extract across all sources */
  maxFacts?: number;

  /** Whether to generate a persistent benchmark from the knowledge */
  generateBenchmark?: boolean;
}
