#!/usr/bin/env tsx
/**
 * CODING CHALLENGE BENCHMARK — E2E TEST
 * ======================================
 *
 * Proves the coding challenge pipeline:
 * 1. Pipeline structure has correct steps (shell + llm)
 * 2. Buggy tests fail independently (baseline)
 * 3. Full pipeline executes via sentinel/run --async=false
 * 4. Parse pipeline output for test results
 * 5. Assert meaningful improvement (score >= 75%)
 *
 * PREREQUISITES:
 *   1. `npm start` running and `./jtag ping` succeeds
 *   2. A cloud LLM provider reachable (for the fix-code LLM step)
 *
 * USAGE:
 *   npx tsx tests/integration/coding-challenge-benchmark.test.ts
 */

import { execSync } from 'child_process';
import { runJtagCommand } from '../test-utils/CRUDTestUtils';
import {
  buildCodingChallengePipeline,
  parseCodingChallengeTestOutput,
  type CodingChallengeConfig,
  type CodingChallengeScore,
} from '../../system/sentinel/pipelines/CodingChallengePipeline';

// ─── Test Configuration ──────────────────────────────────────────────────────

const CHALLENGE_CONFIG: CodingChallengeConfig = {
  challengeDir: 'challenges/task-manager',
  sourceFile: 'task-manager.ts',
  testFile: 'task-manager.test.ts',
  testCommand: 'npx tsx task-manager.test.ts',
};

// ─── Test Phases ─────────────────────────────────────────────────────────────

async function main() {
  console.log('='.repeat(80));
  console.log('CODING CHALLENGE BENCHMARK — E2E TEST');
  console.log('='.repeat(80));
  console.log();

  const results: { phase: string; success: boolean; details: string }[] = [];

  try {
    // ════════════════════════════════════════════════════════════════════════
    // Phase 1: PIPELINE STRUCTURE — Verify pipeline builds correctly
    // ════════════════════════════════════════════════════════════════════════
    console.log('\n' + '─'.repeat(60));
    console.log('Phase 1: PIPELINE STRUCTURE');
    console.log('─'.repeat(60));

    const pipeline = buildCodingChallengePipeline(CHALLENGE_CONFIG);

    console.log(`Pipeline name: ${pipeline.name}`);
    console.log(`Pipeline steps: ${pipeline.steps.length}`);

    const shellSteps = pipeline.steps.filter(s => s.type === 'shell');
    const llmSteps = pipeline.steps.filter(s => s.type === 'llm');

    console.log(`  Shell steps: ${shellSteps.length}`);
    console.log(`  LLM steps: ${llmSteps.length}`);

    // 4 shell steps (read source, read tests, run buggy tests, write fix + run fixed tests)
    // 1 LLM step (fix the code)
    const structureValid = shellSteps.length === 4 && llmSteps.length === 1;
    results.push({
      phase: 'Pipeline Structure',
      success: structureValid,
      details: `${shellSteps.length} shell, ${llmSteps.length} LLM (expected 4 shell, 1 LLM)`,
    });
    console.log(`  Structure valid: ${structureValid}`);

    // ════════════════════════════════════════════════════════════════════════
    // Phase 2: BUGGY BASELINE — Run tests against buggy code independently
    // ════════════════════════════════════════════════════════════════════════
    console.log('\n' + '─'.repeat(60));
    console.log('Phase 2: BUGGY BASELINE (independent test run)');
    console.log('─'.repeat(60));

    let buggyOutput = '';
    try {
      buggyOutput = execSync(
        `npx tsx ${CHALLENGE_CONFIG.testFile}`,
        {
          encoding: 'utf8',
          cwd: CHALLENGE_CONFIG.challengeDir,
          timeout: 30_000,
        }
      );
    } catch (err: any) {
      // Test runner exits non-zero when tests fail — that's expected
      buggyOutput = err.stdout || err.stderr || '';
    }

    const buggyScore = parseCodingChallengeTestOutput(buggyOutput);
    console.log(`  Buggy baseline: ${buggyScore.passed}/${buggyScore.totalTests} passed (${buggyScore.score}%)`);
    console.log(`  Expected: some failures (the challenge has 3 bugs)`);

    const baselineValid = buggyScore.totalTests >= 8 && buggyScore.failed > 0;
    results.push({
      phase: 'Buggy Baseline',
      success: baselineValid,
      details: `${buggyScore.passed} passed, ${buggyScore.failed} failed out of ${buggyScore.totalTests} (score: ${buggyScore.score}%)`,
    });

    // ════════════════════════════════════════════════════════════════════════
    // Phase 3: PIPELINE EXECUTION — Run full pipeline via sentinel
    // ════════════════════════════════════════════════════════════════════════
    console.log('\n' + '─'.repeat(60));
    console.log('Phase 3: PIPELINE EXECUTION (sentinel/run)');
    console.log('─'.repeat(60));

    const pipelineResult = await runJtagCommand(
      `sentinel/run --type=pipeline --async=false --definition='${JSON.stringify(pipeline)}'`
    );

    const execSuccess = Boolean(pipelineResult.success);
    console.log(`  Execution: ${execSuccess ? 'SUCCESS' : 'FAILED'}`);

    if (!execSuccess) {
      console.log(`  Error: ${pipelineResult.error ?? pipelineResult.output ?? 'unknown'}`);
    }

    results.push({
      phase: 'Pipeline Execution',
      success: execSuccess,
      details: execSuccess ? 'Pipeline completed' : `Failed: ${pipelineResult.error ?? 'unknown'}`,
    });

    // ════════════════════════════════════════════════════════════════════════
    // Phase 4: SCORE PARSING — Extract results from pipeline output
    // ════════════════════════════════════════════════════════════════════════
    if (execSuccess) {
      console.log('\n' + '─'.repeat(60));
      console.log('Phase 4: SCORE PARSING');
      console.log('─'.repeat(60));

      const pipelineOutput = String(pipelineResult.output ?? '');
      console.log(`  Pipeline output (last step):`);
      // Show the test output nicely
      for (const line of pipelineOutput.split('\n').slice(0, 20)) {
        console.log(`    ${line}`);
      }

      const fixedScore = parseCodingChallengeTestOutput(pipelineOutput);
      console.log(`\n  Fixed score: ${fixedScore.passed}/${fixedScore.totalTests} passed (${fixedScore.score}%)`);
      console.log(`  Improvement: ${buggyScore.score}% → ${fixedScore.score}%`);

      const scoreValid = fixedScore.totalTests > 0;
      results.push({
        phase: 'Score Parsing',
        success: scoreValid,
        details: `${fixedScore.passed}/${fixedScore.totalTests} passed (${fixedScore.score}%)`,
      });

      // ════════════════════════════════════════════════════════════════════
      // Phase 5: QUALITY GATE — Assert meaningful improvement
      // ════════════════════════════════════════════════════════════════════
      console.log('\n' + '─'.repeat(60));
      console.log('Phase 5: QUALITY GATE');
      console.log('─'.repeat(60));

      const threshold = 75;
      const passedGate = fixedScore.score >= threshold;
      console.log(`  Threshold: ${threshold}%`);
      console.log(`  Achieved:  ${fixedScore.score}%`);
      console.log(`  Result:    ${passedGate ? 'PASS' : 'FAIL'}`);

      if (fixedScore.score === 100) {
        console.log(`  All ${fixedScore.totalTests} tests passed — perfect score!`);
      }

      results.push({
        phase: 'Quality Gate',
        success: passedGate,
        details: `${fixedScore.score}% >= ${threshold}% threshold (improved from ${buggyScore.score}%)`,
      });
    }

  } catch (error) {
    console.error('\nFATAL ERROR:', error);
    results.push({
      phase: 'Fatal',
      success: false,
      details: error instanceof Error ? error.message : String(error),
    });
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // RESULTS SUMMARY
  // ═══════════════════════════════════════════════════════════════════════════
  console.log('\n' + '='.repeat(80));
  console.log('RESULTS SUMMARY');
  console.log('='.repeat(80));

  let allPassed = true;
  for (const r of results) {
    const icon = r.success ? '✅' : '❌';
    console.log(`${icon} ${r.phase}: ${r.details}`);
    if (!r.success) allPassed = false;
  }

  console.log('\n' + '='.repeat(80));
  console.log(allPassed ? '✅ ALL PHASES PASSED' : '❌ SOME PHASES FAILED');
  console.log('='.repeat(80));

  process.exit(allPassed ? 0 : 1);
}

main().catch(err => {
  console.error('Unhandled error:', err);
  process.exit(1);
});
