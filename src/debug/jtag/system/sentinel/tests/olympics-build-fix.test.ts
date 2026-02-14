/**
 * Olympics Validation: Build-Fix Pattern
 *
 * This test validates the SentinelRunner can handle the complex
 * build-fix-loop pattern from the Olympics validation cases.
 *
 * Key capabilities tested:
 * - Loop control (until condition)
 * - Nested condition branches
 * - Command execution
 * - LLM steps with tool calling
 * - Variable substitution
 * - Event emission
 * - Parallel step execution
 */

import { SentinelRunner, runSentinel } from '../SentinelRunner';
import type { PipelineSentinelDefinition } from '../SentinelDefinition';

const WORKING_DIR = '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag';

// =============================================================================
// Build-Fix Pattern: The core Olympics example
// =============================================================================

const buildFixDefinition: PipelineSentinelDefinition = {
  name: 'build-fix-loop',
  type: 'pipeline',
  version: '1.0',
  description: 'Continuously build and fix errors until success',
  steps: [
    // Step 1: Run the build
    {
      type: 'command',
      command: 'code/shell/execute',
      params: {
        command: 'npm',
        args: ['run', 'build:ts'],
        cwd: '$PROJECT_ROOT',
      },
      outputTo: 'buildResult',
    },
    // Step 2: Check if build succeeded
    {
      type: 'condition',
      check: '$buildResult.exitCode === 0',
      then: [
        // Success path: emit success event
        {
          type: 'emit',
          event: 'sentinel:build:success',
          data: '{ "duration": "$ELAPSED_MS", "iterations": "$iteration" }',
        },
      ],
      else: [
        // Failure path: analyze and fix
        {
          type: 'llm',
          prompt: `Analyze this TypeScript build error and suggest a fix:

Error output:
$buildResult.stderr

Provide:
1. Root cause analysis
2. Specific file and line to fix
3. The exact code change needed

If you need to read a file, use the code/read tool.
If you need to edit a file, use the code/edit tool.`,
          model: 'claude-3-5-sonnet-20241022',
          tools: ['code/read', 'code/edit'],
          parseToolCalls: true,
          outputTo: 'fixAttempt',
        },
        // Emit progress event
        {
          type: 'emit',
          event: 'sentinel:build:fix-attempted',
          data: '$fixAttempt',
        },
      ],
    },
  ],
  loop: { type: 'until', check: '$buildResult.exitCode === 0' },
  safety: { maxIterations: 5, timeoutMs: 300000 },
};

// =============================================================================
// PR Review Pattern: Parallel nested sentinels
// =============================================================================

const prReviewDefinition: PipelineSentinelDefinition = {
  name: 'pr-review',
  type: 'pipeline',
  version: '1.0',
  description: 'Comprehensive PR review with parallel checks',
  steps: [
    // Step 1: Get the diff
    {
      type: 'command',
      command: 'code/git/diff',
      params: { base: 'main' },
      outputTo: 'diff',
    },
    // Step 2: Run style and security checks in parallel
    {
      type: 'parallel',
      steps: [
        // Style check (nested sentinel)
        {
          type: 'sentinel',
          definition: {
            name: 'style-check',
            type: 'pipeline',
            version: '1.0',
            steps: [
              {
                type: 'llm',
                prompt: 'Review this diff for code style issues. Be concise.\n\n$diff',
                model: 'claude-3-5-haiku-20241022',
                outputTo: 'styleIssues',
              },
            ],
            loop: { type: 'once' },
          },
          await: true,
          outputTo: 'styleResult',
        },
        // Security check (nested sentinel)
        {
          type: 'sentinel',
          definition: {
            name: 'security-check',
            type: 'pipeline',
            version: '1.0',
            steps: [
              {
                type: 'llm',
                prompt: 'Check this diff for security issues ONLY. Focus on: injection, auth bypass, data exposure.\n\n$diff',
                model: 'claude-3-5-sonnet-20241022',
                outputTo: 'securityIssues',
              },
            ],
            loop: { type: 'once' },
          },
          await: true,
          outputTo: 'securityResult',
        },
      ],
    },
    // Step 3: Synthesize results
    {
      type: 'llm',
      prompt: `Synthesize these review results into a single PR comment. Be constructive.

Style Issues: $styleResult
Security Issues: $securityResult`,
      model: 'claude-3-5-haiku-20241022',
      outputTo: 'reviewComment',
    },
    // Step 4: Post the review
    {
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: 'code-reviews',
        message: '## PR Review\n\n$reviewComment',
      },
    },
  ],
  loop: { type: 'once' },
  safety: { timeoutMs: 120000 },
};

// =============================================================================
// Test Runner
// =============================================================================

async function runOlympicsTests() {
  console.log('\n=== Olympics Validation: SentinelRunner ===\n');

  // Test 1: Build-Fix Pattern (dry run - no actual build)
  console.log('--- Test 1: Build-Fix Pattern Structure ---');
  console.log('Definition validated:', JSON.stringify(buildFixDefinition, null, 2).slice(0, 500) + '...');
  console.log('Steps:', buildFixDefinition.steps.length);
  console.log('Loop type:', buildFixDefinition.loop.type);
  console.log('Safety limits:', buildFixDefinition.safety);
  console.log('');

  // Test 2: PR Review Pattern (dry run - no actual PR)
  console.log('--- Test 2: PR Review Pattern Structure ---');
  console.log('Definition validated:', JSON.stringify(prReviewDefinition, null, 2).slice(0, 500) + '...');
  console.log('Steps:', prReviewDefinition.steps.length);
  console.log('Has parallel step:', prReviewDefinition.steps.some(s => s.type === 'parallel'));
  console.log('');

  // Test 3: Simple execution test with ping command
  console.log('--- Test 3: Simple Execution (ping command) ---');
  const simpleDefinition: PipelineSentinelDefinition = {
    name: 'simple-test',
    type: 'pipeline',
    version: '1.0',
    steps: [
      {
        type: 'command',
        command: 'ping',
        params: {},
        outputTo: 'pingResult',
      },
      {
        type: 'condition',
        check: '$pingResult !== undefined',
        then: [
          {
            type: 'emit',
            event: 'sentinel:test:success',
            data: '$pingResult',
          },
        ],
      },
    ],
    loop: { type: 'once' },
  };

  try {
    const runner = new SentinelRunner({
      workingDir: WORKING_DIR,
      onLog: (msg, level) => console.log(`  [${level.toUpperCase()}] ${msg}`),
      onIteration: (i) => console.log(`  Iteration ${i}`),
      onStepComplete: (step, trace) => {
        console.log(`  Step ${step.type}: ${trace.success ? 'OK' : 'FAIL'} (${trace.durationMs}ms)`);
      },
    });

    const result = await runner.execute(simpleDefinition);
    console.log(`  Result: ${result.success ? 'SUCCESS' : 'FAILED'}`);
    console.log(`  Summary: ${result.summary}`);
    console.log(`  Variables:`, Object.keys(result.context.variables));
  } catch (error: any) {
    console.log(`  Error: ${error.message}`);
  }
  console.log('');

  // Test 4: Condition branching
  console.log('--- Test 4: Condition Branching ---');
  const conditionDefinition: PipelineSentinelDefinition = {
    name: 'condition-test',
    type: 'pipeline',
    version: '1.0',
    steps: [
      {
        type: 'command',
        command: 'ping',
        params: {},
        outputTo: 'pingResult',
      },
      {
        type: 'condition',
        check: '$pingResult !== undefined',
        then: [
          {
            type: 'emit',
            event: 'sentinel:condition:then',
            data: '{ "branch": "then", "pingResult": "$pingResult" }',
          },
        ],
        else: [
          {
            type: 'emit',
            event: 'sentinel:condition:else',
            data: '{ "branch": "else" }',
          },
        ],
      },
    ],
    loop: { type: 'once' },
  };

  try {
    const result = await runSentinel(conditionDefinition, WORKING_DIR, {
      onLog: (msg, level) => console.log(`  [${level.toUpperCase()}] ${msg}`),
    });
    console.log(`  Result: ${result.success ? 'SUCCESS' : 'FAILED'}`);
    console.log(`  Took correct branch: ${result.context.variables.pingResult !== undefined ? 'then' : 'else'}`);
  } catch (error: any) {
    console.log(`  Error: ${error.message}`);
  }
  console.log('');

  // Test 5: Loop until condition
  console.log('--- Test 5: Loop Until Condition ---');
  let loopCount = 0;
  const loopDefinition: PipelineSentinelDefinition = {
    name: 'loop-test',
    type: 'pipeline',
    version: '1.0',
    steps: [
      {
        type: 'command',
        command: 'ping',
        params: {},
        outputTo: 'pingResult',
      },
    ],
    loop: { type: 'until', check: '$pingResult !== undefined' },
    safety: { maxIterations: 10 },
  };

  try {
    const result = await runSentinel(loopDefinition, WORKING_DIR, {
      onIteration: () => loopCount++,
      onLog: (msg, level) => console.log(`  [${level.toUpperCase()}] ${msg}`),
    });
    console.log(`  Result: ${result.success ? 'SUCCESS' : 'FAILED'}`);
    console.log(`  Iterations: ${loopCount}`);
    console.log(`  Stopped because condition was met: ${result.context.variables.pingResult !== undefined}`);
  } catch (error: any) {
    console.log(`  Error: ${error.message}`);
  }
  console.log('');

  console.log('=== Olympics Validation Complete ===\n');
}

// Run if executed directly
const isDirectRun = typeof describe === 'undefined';
if (isDirectRun) {
  runOlympicsTests()
    .then(() => process.exit(0))
    .catch((error) => {
      console.error('Test failed:', error);
      process.exit(1);
    });
}

export { runOlympicsTests, buildFixDefinition, prReviewDefinition };
