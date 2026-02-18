/**
 * CodingChallengePipeline — Deterministic coding challenge evaluation
 *
 * Unlike knowledge benchmarks that use LLM grading (subjective), coding
 * challenges use real test suites for deterministic pass/fail scoring.
 * No LLM grading bias — tests either pass or they don't.
 *
 * Pipeline flow:
 *   Step 0: Shell — Read buggy source code
 *   Step 1: Shell — Read test file
 *   Step 2: Shell — Run tests against buggy code (capture failures)
 *   Step 3: LLM  — Given source + tests + failing output → output corrected source
 *   Step 4: Shell — Copy challenge to temp dir, overwrite source with LLM fix
 *   Step 5: Shell — Run tests against fixed code in temp dir
 *
 * The pipeline's output (step 5) is the final test result string.
 * parseCodingChallengeTestOutput() extracts pass/fail counts from it.
 *
 * Safety: Single-quoted heredoc delimiter (<< 'BOUNDARY') disables all
 * shell expansion, so arbitrary LLM-generated TypeScript (backticks,
 * ${}, etc.) is written literally without escaping issues.
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';

// ============================================================================
// Pipeline Config
// ============================================================================

export interface CodingChallengeConfig {
  /** Path to the challenge directory (e.g., "challenges/task-manager") */
  challengeDir: string;

  /** Source file with intentional bugs (relative to challengeDir) */
  sourceFile: string;

  /** Test file that validates the source (relative to challengeDir) */
  testFile: string;

  /** Command to run tests (default: "npx tsx <testFile>") */
  testCommand?: string;

  /** LLM model for code generation */
  model?: string;

  /** LLM provider */
  provider?: string;
}

// ============================================================================
// Score Types
// ============================================================================

export interface CodingChallengeScore {
  /** Total number of tests in the suite */
  totalTests: number;

  /** Number of tests that passed */
  passed: number;

  /** Number of tests that failed */
  failed: number;

  /** Score as a percentage (0-100) */
  score: number;
}

// ============================================================================
// Pipeline Builder
// ============================================================================

/**
 * Build a sentinel pipeline that evaluates an LLM's ability to fix buggy code.
 *
 * The pipeline reads the buggy source and tests, runs the tests to capture
 * the failing output, asks the LLM to fix the source, writes the fix to a
 * temp directory, and runs the tests again to measure improvement.
 */
export function buildCodingChallengePipeline(config: CodingChallengeConfig): Pipeline {
  const {
    challengeDir,
    sourceFile,
    testFile,
  } = config;

  const testCommand = config.testCommand ?? `npx tsx ${testFile}`;
  // Generate a unique boundary token for the heredoc
  const boundary = 'FIXED_SOURCE_EOF';

  const steps: PipelineStep[] = [
    // Step 0: Read buggy source code
    {
      type: 'shell',
      cmd: `cat ${sourceFile}`,
      workingDir: challengeDir,
    },

    // Step 1: Read test file
    {
      type: 'shell',
      cmd: `cat ${testFile}`,
      workingDir: challengeDir,
    },

    // Step 2: Run tests against buggy code (expect failures; "; true" prevents abort)
    {
      type: 'shell',
      cmd: `${testCommand} 2>&1; true`,
      workingDir: challengeDir,
      timeoutSecs: 30,
    },

    // Step 3: LLM — Fix the buggy source code
    {
      type: 'llm',
      prompt: [
        'You are an expert TypeScript developer. The following source code has bugs.',
        'The test suite below reveals the failures. Fix ALL bugs in the source code.',
        '',
        '=== BUGGY SOURCE CODE ===',
        '{{steps.0.output}}',
        '',
        '=== TEST FILE ===',
        '{{steps.1.output}}',
        '',
        '=== TEST OUTPUT (shows failures) ===',
        '{{steps.2.output}}',
        '',
        'INSTRUCTIONS:',
        '- Output ONLY the corrected source code',
        '- Do NOT include markdown code fences',
        '- Do NOT include explanations',
        '- Keep the same structure, exports, and interface',
        '- Fix ONLY the bugs revealed by failing tests',
      ].join('\n'),
      ...(config.model && { model: config.model }),
      ...(config.provider && { provider: config.provider }),
      temperature: 0.2,
      maxTokens: 4096,
    },

    // Step 4: Copy challenge to temp dir, write LLM fix, and run tests
    // Combined into one step to avoid cross-step newline interpolation issues.
    // Single-quoted heredoc delimiter disables all shell expansion in the body,
    // so arbitrary LLM-generated TypeScript is written literally.
    {
      type: 'shell',
      cmd: [
        `TMPDIR=$(mktemp -d)`,
        `cp -r . "$TMPDIR/"`,
        `cat << '${boundary}' > "$TMPDIR/${sourceFile}"`,
        `{{steps.3.output}}`,
        boundary,
        `cd "$TMPDIR"`,
        `${testCommand} 2>&1; true`,
      ].join('\n'),
      workingDir: challengeDir,
      timeoutSecs: 30,
    },
  ];

  return {
    name: `coding-challenge-${sourceFile.replace(/[^a-z0-9]+/gi, '-')}`,
    steps,
    inputs: { challengeDir, sourceFile, testFile },
  };
}

// ============================================================================
// Score Parser
// ============================================================================

/**
 * Parse test output from the coding challenge test runner.
 *
 * Handles two formats:
 * 1. Summary line: "Results: X passed, Y failed"
 * 2. Individual lines: count checkmarks (✅) and X-marks (❌)
 */
export function parseCodingChallengeTestOutput(output: string): CodingChallengeScore {
  // Try summary line first: "Results: X passed, Y failed"
  const summaryMatch = output.match(/Results:\s*(\d+)\s*passed,\s*(\d+)\s*failed/i);
  if (summaryMatch) {
    const passed = parseInt(summaryMatch[1], 10);
    const failed = parseInt(summaryMatch[2], 10);
    const totalTests = passed + failed;
    return {
      totalTests,
      passed,
      failed,
      score: totalTests > 0 ? Math.round((passed / totalTests) * 100) : 0,
    };
  }

  // Fallback: count checkmarks and X-marks
  const passLines = (output.match(/✅/g) || []).length;
  const failLines = (output.match(/❌/g) || []).length;
  const totalTests = passLines + failLines;

  return {
    totalTests,
    passed: passLines,
    failed: failLines,
    score: totalTests > 0 ? Math.round((passLines / totalTests) * 100) : 0,
  };
}
