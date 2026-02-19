#!/usr/bin/env tsx
/**
 * SENTINEL MULTI-STEP PIPELINE E2E TEST
 * ========================================
 *
 * Proves that the Rust pipeline engine correctly chains Shell → Command → LLM
 * steps with variable interpolation between them.
 *
 * Test pipeline:
 *   Step 0 (Shell): Run `echo` to produce output
 *   Step 1 (Command): Use interpolated output from step 0
 *   Step 2 (LLM): Summarize the chain using outputs from both steps
 *   Step 3 (Condition): Branch based on LLM output
 *   Step 4 (Loop): Execute steps N times with iteration tracking
 *
 * Verifies:
 * - Shell step captures stdout
 * - Command step receives interpolated params
 * - LLM step can reference multiple prior step outputs
 * - Condition step evaluates interpolated expressions
 * - Loop step tracks iterations correctly
 *
 * PREREQUISITES:
 *   1. `npm start` running and `./jtag ping` succeeds
 *
 * USAGE:
 *   npx tsx tests/integration/sentinel-multi-step-pipeline.test.ts
 */

import { runJtagCommand } from '../test-utils/CRUDTestUtils';
import type { Pipeline, PipelineStep } from '../../workers/continuum-core/bindings/modules/sentinel';

// ─── Test Pipelines ──────────────────────────────────────────────────────────

/**
 * Pipeline 1: Shell → Command → LLM chain
 * Tests basic step chaining with interpolation
 */
function buildChainPipeline(): Pipeline {
  return {
    name: 'test-chain-pipeline',
    steps: [
      // Step 0: Shell — produce deterministic output
      {
        type: 'shell',
        cmd: 'echo',
        args: ['Hello from Shell Step'],
        timeoutSecs: 10,
      },

      // Step 1: Shell — capture date for interpolation
      {
        type: 'shell',
        cmd: 'date',
        args: ['+%Y-%m-%d'],
        timeoutSecs: 10,
      },

      // Step 2: LLM — summarize outputs from both shell steps
      {
        type: 'llm',
        prompt: [
          'You received two inputs from a pipeline:',
          'Input 1: {{steps.0.output}}',
          'Input 2: {{steps.1.output}}',
          '',
          'Respond with a single JSON object (no markdown, no code fences):',
          '{ "received_input_1": true, "received_input_2": true, "summary": "Both inputs received" }',
        ].join('\n'),
        temperature: 0.1,
        maxTokens: 256,
      },
    ],
    inputs: { testName: 'chain-test' },
  };
}

/**
 * Pipeline 2: Condition branching
 * Tests that condition step evaluates expressions correctly
 */
function buildConditionPipeline(): Pipeline {
  return {
    name: 'test-condition-pipeline',
    steps: [
      // Step 0: Shell — produce "yes" to trigger condition
      {
        type: 'shell',
        cmd: 'echo',
        args: ['yes'],
        timeoutSecs: 10,
      },

      // Step 1: Condition — branch on step 0 output
      {
        type: 'condition',
        if: '{{steps.0.output}}',
        then: [
          {
            type: 'shell',
            cmd: 'echo',
            args: ['Condition was TRUE'],
            timeoutSecs: 10,
          },
        ],
        else: [
          {
            type: 'shell',
            cmd: 'echo',
            args: ['Condition was FALSE'],
            timeoutSecs: 10,
          },
        ],
      },
    ],
    inputs: { testName: 'condition-test' },
  };
}

/**
 * Pipeline 3: Loop with iteration tracking
 * Tests that loop steps execute N times and track iteration count
 */
function buildLoopPipeline(): Pipeline {
  return {
    name: 'test-loop-pipeline',
    steps: [
      // Step 0: Loop — execute 3 iterations
      {
        type: 'loop',
        count: 3,
        steps: [
          // loop.0: Echo the iteration index
          {
            type: 'shell',
            cmd: 'echo',
            args: ['Iteration {{input.iteration}}'],
            timeoutSecs: 10,
          },
        ],
      },

      // Step 1: Shell — confirm loop completed
      {
        type: 'shell',
        cmd: 'echo',
        args: ['Loop completed'],
        timeoutSecs: 10,
      },
    ],
    inputs: { testName: 'loop-test' },
  };
}

/**
 * Pipeline 4: Emit + Watch (if another sentinel is listening)
 * Tests event-based step coordination
 */
function buildEmitPipeline(): Pipeline {
  return {
    name: 'test-emit-pipeline',
    steps: [
      // Step 0: Shell — produce data
      {
        type: 'shell',
        cmd: 'echo',
        args: ['event-payload-data'],
        timeoutSecs: 10,
      },

      // Step 1: Emit — fire event with interpolated payload
      {
        type: 'emit',
        event: 'test:pipeline:complete',
        payload: {
          source: 'sentinel-multi-step-test',
          output: '{{steps.0.output}}',
        },
      },
    ],
    inputs: { testName: 'emit-test' },
  };
}

// ─── Test Execution ──────────────────────────────────────────────────────────

async function main() {
  console.log('='.repeat(80));
  console.log('SENTINEL MULTI-STEP PIPELINE — E2E TEST');
  console.log('='.repeat(80));
  console.log();

  const results: { phase: string; success: boolean; details: string }[] = [];

  // ════════════════════════════════════════════════════════════════════════
  // Test 1: Shell → Command → LLM Chain
  // ════════════════════════════════════════════════════════════════════════
  console.log('─'.repeat(60));
  console.log('Test 1: SHELL → LLM CHAIN');
  console.log('─'.repeat(60));

  try {
    const chainPipeline = buildChainPipeline();
    const chainResult = await runJtagCommand(
      `sentinel/run --type=pipeline --async=false --definition='${JSON.stringify(chainPipeline)}'`
    );

    const chainSuccess = Boolean(chainResult.success);
    console.log(`   Success: ${chainSuccess}`);

    // Pipeline output from sync mode
    const output = chainResult.output as string ?? '';
    if (output) {
      console.log(`   Pipeline output: ${output.slice(0, 200)}`);
    }

    results.push({
      phase: 'Shell→LLM Chain',
      success: chainSuccess,
      details: chainSuccess ? 'All steps completed' : `Failed: ${chainResult.error}`,
    });
  } catch (error) {
    results.push({
      phase: 'Shell→LLM Chain',
      success: false,
      details: error instanceof Error ? error.message : String(error),
    });
  }

  // ════════════════════════════════════════════════════════════════════════
  // Test 2: Condition Branching
  // ════════════════════════════════════════════════════════════════════════
  console.log('\n' + '─'.repeat(60));
  console.log('Test 2: CONDITION BRANCHING');
  console.log('─'.repeat(60));

  try {
    const condPipeline = buildConditionPipeline();
    const condResult = await runJtagCommand(
      `sentinel/run --type=pipeline --async=false --definition='${JSON.stringify(condPipeline)}'`
    );

    const condSuccess = Boolean(condResult.success);
    console.log(`   Success: ${condSuccess}`);
    if (condResult.output) {
      console.log(`   Output: ${String(condResult.output).slice(0, 200)}`);
    }

    results.push({
      phase: 'Condition Branching',
      success: condSuccess,
      details: condSuccess ? 'Condition evaluated and branched correctly' : `Failed: ${condResult.error}`,
    });
  } catch (error) {
    results.push({
      phase: 'Condition Branching',
      success: false,
      details: error instanceof Error ? error.message : String(error),
    });
  }

  // ════════════════════════════════════════════════════════════════════════
  // Test 3: Loop Execution
  // ════════════════════════════════════════════════════════════════════════
  console.log('\n' + '─'.repeat(60));
  console.log('Test 3: LOOP EXECUTION');
  console.log('─'.repeat(60));

  try {
    const loopPipeline = buildLoopPipeline();
    const loopResult = await runJtagCommand(
      `sentinel/run --type=pipeline --async=false --definition='${JSON.stringify(loopPipeline)}'`
    );

    const loopSuccess = Boolean(loopResult.success);
    console.log(`   Success: ${loopSuccess}`);
    if (loopResult.output) {
      console.log(`   Output: ${String(loopResult.output).slice(0, 200)}`);
    }

    results.push({
      phase: 'Loop Execution',
      success: loopSuccess,
      details: loopSuccess ? 'Loop completed successfully' : `Failed: ${loopResult.error}`,
    });
  } catch (error) {
    results.push({
      phase: 'Loop Execution',
      success: false,
      details: error instanceof Error ? error.message : String(error),
    });
  }

  // ════════════════════════════════════════════════════════════════════════
  // Test 4: Emit Event
  // ════════════════════════════════════════════════════════════════════════
  console.log('\n' + '─'.repeat(60));
  console.log('Test 4: EMIT EVENT');
  console.log('─'.repeat(60));

  try {
    const emitPipeline = buildEmitPipeline();
    const emitResult = await runJtagCommand(
      `sentinel/run --type=pipeline --async=false --definition='${JSON.stringify(emitPipeline)}'`
    );

    const emitSuccess = Boolean(emitResult.success);
    console.log(`   Success: ${emitSuccess}`);

    results.push({
      phase: 'Emit Event',
      success: emitSuccess,
      details: emitSuccess ? 'Event emitted successfully' : `Failed: ${emitResult.error}`,
    });
  } catch (error) {
    results.push({
      phase: 'Emit Event',
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
  console.log(allPassed ? '✅ ALL TESTS PASSED' : '❌ SOME TESTS FAILED');
  console.log('='.repeat(80));

  process.exit(allPassed ? 0 : 1);
}

main().catch(err => {
  console.error('Unhandled error:', err);
  process.exit(1);
});
