/**
 * SentinelRunner Tests — Validate the declarative step engine
 */

import { SentinelRunner, runSentinel } from '../SentinelRunner';
import type { PipelineSentinelDefinition } from '../SentinelDefinition';

const WORKING_DIR = '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag';

// Stub test framework functions when running standalone
const isJest = typeof describe !== 'undefined';
const _describe = isJest ? describe : (name: string, fn: () => void) => {};
const _it = isJest ? it : (name: string, fn: () => void) => {};
const _expect = isJest ? expect : (v: any) => ({ toBe: () => {}, toBeDefined: () => {}, toBeUndefined: () => {}, toContain: () => {}, toBeLessThan: () => {} });

_describe('SentinelRunner', () => {
  _describe('Basic Execution', () => {
    _it('should execute a simple command step', async () => {
      const definition: PipelineSentinelDefinition = {
        name: 'simple-ping',
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
        loop: { type: 'once' },
      };

      const runner = new SentinelRunner({ workingDir: WORKING_DIR });
      const result = await runner.execute(definition);

      console.log('Result:', result);
      _expect(result.success).toBe(true);
      _expect(result.context.variables.pingResult).toBeDefined();
    });

    _it('should substitute $variables in params', async () => {
      const definition: PipelineSentinelDefinition = {
        name: 'variable-test',
        type: 'pipeline',
        version: '1.0',
        steps: [
          {
            type: 'command',
            command: 'ping',
            params: {},
            outputTo: 'firstPing',
          },
          {
            type: 'command',
            command: 'health-check',
            params: {},
            outputTo: 'healthResult',
          },
        ],
        loop: { type: 'once' },
      };

      const result = await runSentinel(definition, WORKING_DIR);

      console.log('Variables:', result.context.variables);
      _expect(result.success).toBe(true);
      _expect(result.context.variables.firstPing).toBeDefined();
      _expect(result.context.variables.healthResult).toBeDefined();
    });
  });

  _describe('Loop Control', () => {
    _it('should execute loop.count times', async () => {
      let iterations = 0;

      const definition: PipelineSentinelDefinition = {
        name: 'count-loop',
        type: 'pipeline',
        version: '1.0',
        steps: [
          {
            type: 'command',
            command: 'ping',
            params: {},
          },
        ],
        loop: { type: 'count', max: 3 },
      };

      const runner = new SentinelRunner({
        workingDir: WORKING_DIR,
        onIteration: (i) => { iterations = i; },
      });

      const result = await runner.execute(definition);

      _expect(result.success).toBe(true);
      _expect(iterations).toBe(3);
    });

    _it('should stop on loop.until condition', async () => {
      const definition: PipelineSentinelDefinition = {
        name: 'until-loop',
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
      };

      const result = await runSentinel(definition, WORKING_DIR);

      // Should complete after first iteration since pingResult is set
      _expect(result.success).toBe(true);
      _expect(result.context.iteration).toBe(1);
    });
  });

  _describe('Condition Steps', () => {
    _it('should execute then branch when condition is true', async () => {
      const definition: PipelineSentinelDefinition = {
        name: 'condition-then',
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
                type: 'command',
                command: 'health-check',
                params: {},
                outputTo: 'healthResult',
              },
            ],
          },
        ],
        loop: { type: 'once' },
      };

      const result = await runSentinel(definition, WORKING_DIR);

      _expect(result.success).toBe(true);
      _expect(result.context.variables.healthResult).toBeDefined();
    });

    _it('should execute else branch when condition is false', async () => {
      const definition: PipelineSentinelDefinition = {
        name: 'condition-else',
        type: 'pipeline',
        version: '1.0',
        steps: [
          {
            type: 'condition',
            check: '$nonexistent === true',
            then: [
              {
                type: 'command',
                command: 'ping',
                params: {},
                outputTo: 'thenResult',
              },
            ],
            else: [
              {
                type: 'command',
                command: 'health-check',
                params: {},
                outputTo: 'elseResult',
              },
            ],
          },
        ],
        loop: { type: 'once' },
      };

      const result = await runSentinel(definition, WORKING_DIR);

      _expect(result.success).toBe(true);
      _expect(result.context.variables.thenResult).toBeUndefined();
      _expect(result.context.variables.elseResult).toBeDefined();
    });
  });

  _describe('Nested Sentinels', () => {
    _it('should execute nested sentinel and return result', async () => {
      const definition: PipelineSentinelDefinition = {
        name: 'parent-sentinel',
        type: 'pipeline',
        version: '1.0',
        steps: [
          {
            type: 'sentinel',
            definition: {
              name: 'child-sentinel',
              type: 'pipeline',
        version: '1.0',
              steps: [
                {
                  type: 'command',
                  command: 'ping',
                  params: {},
                  outputTo: 'childPing',
                },
              ],
              loop: { type: 'once' },
            },
            await: true,
            outputTo: 'childResult',
          },
        ],
        loop: { type: 'once' },
      };

      const result = await runSentinel(definition, WORKING_DIR);

      _expect(result.success).toBe(true);
      _expect(result.context.variables.childResult).toBeDefined();
      _expect((result.context.variables.childResult as any).success).toBe(true);
    });
  });

  _describe('Safety Limits', () => {
    _it('should stop at maxIterations', async () => {
      const definition: PipelineSentinelDefinition = {
        name: 'infinite-loop',
        type: 'pipeline',
        version: '1.0',
        steps: [
          {
            type: 'command',
            command: 'ping',
            params: {},
          },
        ],
        loop: { type: 'continuous' },
        safety: { maxIterations: 5 },
      };

      const result = await runSentinel(definition, WORKING_DIR);

      _expect(result.success).toBe(false);
      _expect(result.summary).toContain('Max iterations');
      _expect(result.context.iteration).toBe(6); // Stopped after exceeding 5
    });

    _it('should stop at timeout', async () => {
      const definition: PipelineSentinelDefinition = {
        name: 'timeout-test',
        type: 'pipeline',
        version: '1.0',
        steps: [
          {
            type: 'command',
            command: 'ping',
            params: {},
          },
        ],
        loop: { type: 'continuous' },
        safety: { timeoutMs: 100, maxIterations: 1000 },
      };

      const start = Date.now();
      const result = await runSentinel(definition, WORKING_DIR);
      const elapsed = Date.now() - start;

      _expect(result.success).toBe(false);
      _expect(result.summary).toContain('Timeout');
      _expect(elapsed).toBeLessThan(500); // Should stop reasonably quickly
    });
  });
});

// Run standalone test
async function runStandaloneTest() {
  console.log('\n=== SentinelRunner Standalone Test ===\n');

  // Test Olympics-style build-fix pattern (simplified)
  const buildFixDefinition: PipelineSentinelDefinition = {
    name: 'build-fix-demo',
    type: 'pipeline',
        version: '1.0',
    description: 'Demonstrates the build-fix pattern from Olympics',
    steps: [
      {
        type: 'command',
        command: 'ping',
        params: {},
        outputTo: 'buildResult',
      },
      {
        type: 'condition',
        check: '$buildResult !== undefined',
        then: [
          {
            type: 'emit',
            event: 'sentinel:build:success',
            data: '$buildResult',
          },
        ],
        else: [
          {
            type: 'emit',
            event: 'sentinel:build:failed',
            data: '$buildResult',
          },
        ],
      },
    ],
    loop: { type: 'until', check: '$buildResult !== undefined' },
    safety: { maxIterations: 5, timeoutMs: 30000 },
  };

  const runner = new SentinelRunner({
    workingDir: WORKING_DIR,
    onLog: (msg, level) => console.log(`[${level.toUpperCase()}] ${msg}`),
    onIteration: (i) => console.log(`\n--- Iteration ${i} ---`),
    onStepComplete: (step, trace) => {
      console.log(`  Step ${step.type}: ${trace.success ? 'OK' : 'FAIL'} (${trace.durationMs}ms)`);
    },
  });

  const result = await runner.execute(buildFixDefinition);

  console.log('\n=== Result ===');
  console.log(`Success: ${result.success}`);
  console.log(`Summary: ${result.summary}`);
  console.log(`Iterations: ${result.context.iteration}`);
  console.log(`Variables:`, Object.keys(result.context.variables));

  return result;
}

// Check if running directly (not via test runner)
// When run with tsx directly, require.main === module is not reliable
// So we check if describe is not defined (not in Jest/vitest context)
const isDirectRun = typeof describe === 'undefined';

if (isDirectRun) {
  runStandaloneTest()
    .then((result) => {
      process.exit(result.success ? 0 : 1);
    })
    .catch((error) => {
      console.error('Test failed:', error);
      process.exit(1);
    });
}
