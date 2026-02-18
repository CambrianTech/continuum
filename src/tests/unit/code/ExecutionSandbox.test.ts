/**
 * ExecutionSandbox Unit Tests
 *
 * Tests process-isolated code execution:
 * - Command allowlist enforcement
 * - Successful execution with output capture
 * - Timeout enforcement (SIGTERM → SIGKILL)
 * - Output size truncation
 * - Restricted environment variables
 * - Spawn error handling
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ExecutionSandbox, type SandboxConfig, type SandboxResult } from '../../../system/code/server/ExecutionSandbox';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

// Mock Logger
vi.mock('../../../system/core/logging/Logger', () => ({
  Logger: {
    create: () => ({
      debug: () => {},
      info: () => {},
      warn: () => {},
      error: () => {},
    }),
  },
}));

function makeConfig(overrides?: Partial<SandboxConfig>): SandboxConfig {
  return {
    command: 'node',
    args: ['-e', 'console.log("hello")'],
    cwd: '/tmp',
    timeoutMs: 5000,
    maxOutputBytes: 10240,
    personaId: 'test-persona-0001' as UUID,
    ...overrides,
  };
}

describe('ExecutionSandbox', () => {
  let sandbox: ExecutionSandbox;

  beforeEach(() => {
    sandbox = new ExecutionSandbox();
  });

  describe('command allowlist', () => {
    it('rejects commands not in allowlist', async () => {
      const config = makeConfig({ command: 'rm' });
      const result = await sandbox.execute(config);

      expect(result.success).toBe(false);
      expect(result.exitCode).toBe(-1);
      expect(result.error).toContain('not in the sandbox allowlist');
      expect(result.error).toContain('rm');
    });

    it('rejects arbitrary shell commands', async () => {
      const config = makeConfig({ command: 'bash' });
      const result = await sandbox.execute(config);

      expect(result.success).toBe(false);
      expect(result.error).toContain('not in the sandbox allowlist');
    });

    it('rejects curl/wget', async () => {
      for (const cmd of ['curl', 'wget']) {
        const config = makeConfig({ command: cmd });
        const result = await sandbox.execute(config);
        expect(result.success).toBe(false);
        expect(result.error).toContain('not in the sandbox allowlist');
      }
    });

    it('allows node', async () => {
      const config = makeConfig({ command: 'node', args: ['-e', 'process.exit(0)'] });
      const result = await sandbox.execute(config);
      // May fail if node not at expected path, but should NOT fail with allowlist error
      expect(result.error ?? '').not.toContain('not in the sandbox allowlist');
    });

    it('allows npx', async () => {
      const config = makeConfig({ command: 'npx', args: ['--version'] });
      const result = await sandbox.execute(config);
      expect(result.error ?? '').not.toContain('not in the sandbox allowlist');
    });

    it('allows tsc', async () => {
      const config = makeConfig({ command: 'tsc', args: ['--version'] });
      const result = await sandbox.execute(config);
      expect(result.error ?? '').not.toContain('not in the sandbox allowlist');
    });

    it('allows npm', async () => {
      const config = makeConfig({ command: 'npm', args: ['--version'] });
      const result = await sandbox.execute(config);
      expect(result.error ?? '').not.toContain('not in the sandbox allowlist');
    });

    it('extracts basename for path commands', async () => {
      // /usr/local/bin/node should still match "node" in allowlist
      const config = makeConfig({ command: '/usr/local/bin/node', args: ['-e', 'process.exit(0)'] });
      const result = await sandbox.execute(config);
      expect(result.error ?? '').not.toContain('not in the sandbox allowlist');
    });
  });

  describe('successful execution', () => {
    it('captures stdout', async () => {
      const config = makeConfig({
        command: 'node',
        args: ['-e', 'console.log("sandbox-output")'],
      });
      const result = await sandbox.execute(config);

      expect(result.success).toBe(true);
      expect(result.exitCode).toBe(0);
      expect(result.stdout).toContain('sandbox-output');
      expect(result.timedOut).toBe(false);
      expect(result.truncated).toBe(false);
    });

    it('captures stderr', async () => {
      const config = makeConfig({
        command: 'node',
        args: ['-e', 'console.error("err-msg")'],
      });
      const result = await sandbox.execute(config);

      expect(result.exitCode).toBe(0);
      expect(result.stderr).toContain('err-msg');
    });

    it('tracks duration', async () => {
      const config = makeConfig({
        command: 'node',
        args: ['-e', 'setTimeout(() => {}, 50)'],
      });
      const result = await sandbox.execute(config);

      expect(result.durationMs).toBeGreaterThan(0);
    });

    it('reports non-zero exit code as failure', async () => {
      const config = makeConfig({
        command: 'node',
        args: ['-e', 'process.exit(42)'],
      });
      const result = await sandbox.execute(config);

      expect(result.success).toBe(false);
      expect(result.exitCode).toBe(42);
      expect(result.timedOut).toBe(false);
    });
  });

  describe('timeout enforcement', () => {
    it('kills process on timeout', async () => {
      const config = makeConfig({
        command: 'node',
        args: ['-e', 'setTimeout(() => {}, 60000)'], // Would run 60s
        timeoutMs: 500, // Kill after 500ms
      });
      const result = await sandbox.execute(config);

      expect(result.success).toBe(false);
      expect(result.timedOut).toBe(true);
      expect(result.error).toContain('Timed out');
    }, 10_000);
  });

  describe('output size limits', () => {
    it('truncates output exceeding maxOutputBytes', async () => {
      // Generate output larger than limit
      const config = makeConfig({
        command: 'node',
        args: ['-e', `for(let i=0;i<500;i++) console.log("x".repeat(100))`],
        maxOutputBytes: 1024, // 1KB limit
      });
      const result = await sandbox.execute(config);

      expect(result.truncated).toBe(true);
      // stdout should be capped near maxOutputBytes
      expect(result.stdout.length).toBeLessThanOrEqual(1200); // some tolerance
    });
  });

  describe('environment isolation', () => {
    it('sets SANDBOX_EXECUTION env var', async () => {
      const config = makeConfig({
        command: 'node',
        args: ['-e', 'console.log(process.env.SANDBOX_EXECUTION)'],
      });
      const result = await sandbox.execute(config);

      expect(result.stdout).toContain('true');
    });

    it('sets NODE_ENV to sandbox', async () => {
      const config = makeConfig({
        command: 'node',
        args: ['-e', 'console.log(process.env.NODE_ENV)'],
      });
      const result = await sandbox.execute(config);

      expect(result.stdout).toContain('sandbox');
    });

    it('sets PERSONA_ID', async () => {
      const config = makeConfig({
        command: 'node',
        args: ['-e', 'console.log(process.env.PERSONA_ID)'],
        personaId: 'test-persona-xyz' as UUID,
      });
      const result = await sandbox.execute(config);

      expect(result.stdout).toContain('test-persona-xyz');
    });

    it('restricts PATH', async () => {
      const config = makeConfig({
        command: 'node',
        args: ['-e', 'console.log(process.env.PATH)'],
      });
      const result = await sandbox.execute(config);

      // PATH should only contain restricted locations
      const pathDirs = result.stdout.trim().split(':');
      const allowedDirs = ['/opt/homebrew/bin', '/usr/local/bin', '/usr/bin', '/bin'];
      for (const dir of pathDirs) {
        expect(allowedDirs).toContain(dir);
      }
    });

    it('merges custom env vars', async () => {
      const config = makeConfig({
        command: 'node',
        args: ['-e', 'console.log(process.env.CUSTOM_VAR)'],
        env: { CUSTOM_VAR: 'test-value' },
      });
      const result = await sandbox.execute(config);

      expect(result.stdout).toContain('test-value');
    });
  });

  describe('result structure', () => {
    it('returns all required fields on success', async () => {
      const config = makeConfig({
        command: 'node',
        args: ['-e', 'console.log("ok")'],
      });
      const result = await sandbox.execute(config);

      expect(result).toHaveProperty('success');
      expect(result).toHaveProperty('exitCode');
      expect(result).toHaveProperty('stdout');
      expect(result).toHaveProperty('stderr');
      expect(result).toHaveProperty('durationMs');
      expect(result).toHaveProperty('truncated');
      expect(result).toHaveProperty('timedOut');
      expect(typeof result.success).toBe('boolean');
      expect(typeof result.exitCode).toBe('number');
      expect(typeof result.stdout).toBe('string');
      expect(typeof result.stderr).toBe('string');
      expect(typeof result.durationMs).toBe('number');
      expect(typeof result.truncated).toBe('boolean');
      expect(typeof result.timedOut).toBe('boolean');
    });

    it('returns all required fields on allowlist rejection', async () => {
      const config = makeConfig({ command: 'forbidden-cmd' });
      const result = await sandbox.execute(config);

      expect(result.success).toBe(false);
      expect(result.exitCode).toBe(-1);
      expect(result.stdout).toBe('');
      expect(result.stderr).toBe('');
      expect(result.durationMs).toBe(0);
      expect(result.truncated).toBe(false);
      expect(result.timedOut).toBe(false);
      expect(result.error).toBeTruthy();
    });
  });
});
