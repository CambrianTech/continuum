/**
 * BenchmarkPipeline — Builds a sentinel pipeline that generates a persistent
 * benchmark (test suite) from SourceKnowledge.
 *
 * A benchmark is a set of questions with expected answers and rubrics,
 * derived from extracted facts. Benchmarks are PERSISTENT — stored in the
 * data layer, reusable across sessions and personas.
 *
 * Pipeline flow:
 *   Step 0: LLM — Generate benchmark questions from SourceKnowledge
 *   Step 1: Command — data/create to persist BenchmarkDefinition
 *   Step 2: Emit — benchmark:ready event
 *
 * Benchmarks serve two purposes:
 * 1. Measure baseline knowledge (before training)
 * 2. Validate improvement (after training)
 *
 * The BenchmarkRunner (separate pipeline) executes a persona against
 * a benchmark and records BenchmarkResult.
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';

// ============================================================================
// Pipeline Config
// ============================================================================

export interface BenchmarkPipelineConfig {
  /** Domain name for the benchmark (e.g., "nexaflux-corporation") */
  domain: string;

  /** Human-readable benchmark name */
  name: string;

  /** Source knowledge to generate benchmark from (JSON string or interpolation ref) */
  sourceKnowledge: string;

  /** Number of questions to generate (default: 30) */
  questionCount?: number;

  /** LLM model for question generation */
  model?: string;

  /** LLM provider for question generation */
  provider?: string;
}

// ============================================================================
// Pipeline Builder
// ============================================================================

/**
 * Build a sentinel pipeline that generates a persistent benchmark
 * from source knowledge.
 */
export function buildBenchmarkPipeline(config: BenchmarkPipelineConfig): Pipeline {
  const {
    domain,
    name,
    sourceKnowledge,
    questionCount = 30,
  } = config;

  const steps: PipelineStep[] = [
    // Step 0: Generate benchmark questions from source knowledge
    {
      type: 'llm',
      prompt: [
        'You are a benchmark designer. Given the following source knowledge,',
        `generate ${questionCount} questions that thoroughly test understanding of the material.`,
        '',
        'SOURCE KNOWLEDGE:',
        sourceKnowledge,
        '',
        'REQUIREMENTS:',
        '- Cover all categories found in the source knowledge',
        '- Mix difficulty levels: ~30% easy, ~40% medium, ~30% hard',
        '- Each question should test a specific fact or concept',
        '- Expected answers should be precise and verifiable',
        '- Rubrics should describe what a good answer includes',
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "questions": [',
        '    {',
        '      "question": "What is the name of the CEO?",',
        '      "expectedAnswer": "Dr. Elena Vasquez",',
        '      "rubric": "Must name the correct CEO. Partial credit for knowing it is a doctor.",',
        '      "category": "people",',
        '      "difficulty": "easy",',
        '      "factIndices": [0, 3]',
        '    }',
        '  ]',
        '}',
      ].join('\n'),
      ...(config.model && { model: config.model }),
      ...(config.provider && { provider: config.provider }),
      temperature: 0.5,
      maxTokens: 8192,
    },

    // Step 1: Persist benchmark to database
    {
      type: 'command',
      command: 'data/create',
      params: {
        collection: 'academy_benchmarks',
        data: {
          name,
          domain,
          questions: '{{steps.0.output.questions}}',
          knowledgeSummary: `Auto-generated benchmark for domain "${domain}"`,
          factCount: questionCount,
          createdBy: 'BenchmarkPipeline',
          version: 1,
        },
      },
    },

    // Step 2: Emit benchmark:ready event
    {
      type: 'emit',
      event: `benchmark:ready:${domain}`,
      payload: {
        benchmarkId: '{{steps.1.data.data.id}}',
        domain,
        name,
        questionCount: '{{steps.0.output.questions.length}}',
      },
    },
  ];

  return {
    name: `benchmark-generate-${domain}`,
    steps,
    inputs: { domain, name, questionCount },
  };
}

// ============================================================================
// Benchmark Runner Pipeline
// ============================================================================

export interface BenchmarkRunnerConfig {
  /** Benchmark ID to run */
  benchmarkId: string;

  /** Persona to test */
  personaId: string;

  /** Persona name */
  personaName: string;

  /** Adapter ID to use (optional — tests base model if absent) */
  adapterId?: string;

  /** LLM model for answering and grading */
  model?: string;

  /** LLM provider */
  provider?: string;
}

/**
 * Build a pipeline that runs a persona against a benchmark and records results.
 *
 * Flow:
 *   Step 0: Command — data/read benchmark to get questions
 *   Step 1: LLM — Answer all questions as the persona
 *   Step 2: LLM — Grade answers against expected answers and rubrics
 *   Step 3: Command — data/create to persist BenchmarkResult
 *   Step 4: Emit — benchmark:scored
 */
export function buildBenchmarkRunnerPipeline(config: BenchmarkRunnerConfig): Pipeline {
  const { benchmarkId, personaId, personaName } = config;

  const steps: PipelineStep[] = [
    // Step 0: Load the benchmark definition (use data/list + filter because
    // academy_benchmarks is a dynamic collection without a registered entity)
    {
      type: 'command',
      command: 'data/list',
      params: {
        collection: 'academy_benchmarks',
        filter: { id: benchmarkId },
        limit: 1,
      },
    },

    // Step 1: Answer all questions as the persona
    {
      type: 'llm',
      prompt: [
        `You are ${personaName}. Answer the following questions to the best of your ability.`,
        'Be precise and thorough. Answer each question separately.',
        '',
        'Questions:',
        '{{steps.0.data.items.0.questions}}',
        '',
        'Output ONLY a JSON array of answers (no markdown, no code fences):',
        '[',
        '  { "questionIndex": 0, "answer": "Your answer here" }',
        ']',
      ].join('\n'),
      ...(config.model && { model: config.model }),
      ...(config.provider && { provider: config.provider }),
      temperature: 0.3,
      maxTokens: 8192,
    },

    // Step 2: Grade answers
    {
      type: 'llm',
      prompt: [
        'Grade the following answers against the benchmark questions.',
        'Score each answer 0-100 based on accuracy and completeness.',
        '',
        'Questions with expected answers and rubrics:',
        '{{steps.0.data.items.0.questions}}',
        '',
        'Student answers:',
        '{{steps.1.output}}',
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "overallScore": <0-100 weighted average>,',
        '  "questionScores": [',
        '    {',
        '      "questionIndex": 0,',
        '      "score": <0-100>,',
        '      "answer": "the student answer verbatim",',
        '      "feedback": "why this score"',
        '    }',
        '  ],',
        '  "categoryScores": {',
        '    "category-name": <0-100 average for category>',
        '  }',
        '}',
      ].join('\n'),
      ...(config.model && { model: config.model }),
      ...(config.provider && { provider: config.provider }),
      temperature: 0.2,
      maxTokens: 8192,
    },

    // Step 3: Persist benchmark result
    {
      type: 'command',
      command: 'data/create',
      params: {
        collection: 'academy_benchmark_results',
        data: {
          benchmarkId,
          benchmarkName: '{{steps.0.data.items.0.name}}',
          personaId,
          personaName,
          overallScore: '{{steps.2.output.overallScore}}',
          questionScores: '{{steps.2.output.questionScores}}',
          categoryScores: '{{steps.2.output.categoryScores}}',
          ...(config.adapterId && { adapterId: config.adapterId }),
          runAt: new Date().toISOString(),
        },
      },
    },

    // Step 4: Emit scored event
    {
      type: 'emit',
      event: `benchmark:scored:${benchmarkId}`,
      payload: {
        benchmarkId,
        personaId,
        personaName,
        overallScore: '{{steps.2.output.overallScore}}',
        resultId: '{{steps.3.data.data.id}}',
      },
    },
  ];

  return {
    name: `benchmark-run-${personaName.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
    steps,
    inputs: { benchmarkId, personaId, personaName },
  };
}
